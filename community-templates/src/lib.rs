use crate::cli::ObserverWardConfig;
use crossterm::style::Stylize;
use error::Error;
use futures::channel::mpsc::unbounded;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use observer_ward_what_server::{NmapFingerPrint, WhatServer};
use observer_ward_what_web::fingerprint::WebFingerPrint;
use observer_ward_what_web::{RequestOption, TemplateResult, WhatWeb, WhatWebResult};
use once_cell::sync::Lazy;
use prettytable::csv::Reader;
use prettytable::{color, Attr, Cell, Row, Table};
use reqwest::redirect::Policy;
use reqwest::{header, Proxy};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::io::{BufRead, Read};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

pub mod api;
pub mod cli;
pub mod error;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VerifyWebFingerPrint {
    name: String,
    priority: u32,
    fingerprint: Vec<WebFingerPrint>,
}

pub fn print_what_web(what_web_result: &WhatWebResult) {
    let color_web_name: Vec<String> = what_web_result.name.iter().map(String::from).collect();
    let status_code =
        reqwest::StatusCode::from_u16(what_web_result.status_code).unwrap_or_default();
    if !what_web_result.name.is_empty() {
        print!("[ {} |", what_web_result.url);
        print!("{}", format!("{:?}", color_web_name).green());
        print!(" | {} | ", what_web_result.length);
        if status_code.is_success() {
            print!("{}", status_code.as_u16().to_string().green());
        } else {
            print!("{}", status_code.as_u16().to_string().red());
        }
        println!(" | {} ]", what_web_result.title);
    } else {
        println!(
            "[ {} | {:?} | {} | {} | {} ]",
            what_web_result.url,
            color_web_name,
            what_web_result.length,
            what_web_result.status_code,
            what_web_result.title,
        );
    }
}

pub async fn webhook_results(
    what_web_result: WhatWebResult,
    webhook_url: &str,
    webhook_auth: &Option<String>,
) -> WhatWebResult {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    let ua = "Mozilla/5.0 (X11; Linux x86_64; rv:94.0) Gecko/20100101 Firefox/94.0";
    headers.insert(header::USER_AGENT, header::HeaderValue::from_static(ua));
    if let Some(wa) = webhook_auth {
        let h = header::HeaderValue::from_str(wa);
        headers.insert(
            header::AUTHORIZATION,
            h.unwrap_or(header::HeaderValue::from_static("AUTHORIZATION")),
        );
    }
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .pool_max_idle_per_host(0)
        .danger_accept_invalid_certs(true)
        .redirect(Policy::none())
        .timeout(Duration::new(10, 0));
    let what_web_result_json = json!(what_web_result)
        .as_object()
        .unwrap_or(&serde_json::Map::new())
        .clone();
    let _: Result<_, _> = client
        .build()
        .unwrap_or_default()
        .post(webhook_url)
        .json(&what_web_result_json)
        .send()
        .await;
    what_web_result.clone()
}

pub fn print_opening() {
    let s = r#" __     __     ______     ______     _____
/\ \  _ \ \   /\  __ \   /\  == \   /\  __-.
\ \ \/ ".\ \  \ \  __ \  \ \  __<   \ \ \/\ \
 \ \__/".~\_\  \ \_\ \_\  \ \_\ \_\  \ \____-
  \/_/   \/_/   \/_/\/_/   \/_/ /_/   \/____/
Community based web fingerprint analysis tool."#;
    println!("{}", s.green());
    let info = r#"_____________________________________________
:  https://github.com/0x727/FingerprintHub  :
:  https://github.com/0x727/ObserverWard    :
 --------------------------------------------"#;
    println!("{}", info.yellow());
}

pub struct Helper<'a> {
    request_option: RequestOption,
    config_path: &'a PathBuf,
    config: &'a ObserverWardConfig,
    msg: HashMap<String, String>,
}

static OBSERVER_WARD_PATH: Lazy<PathBuf> = Lazy::new(|| -> PathBuf {
    let mut config_path = PathBuf::new();
    if let Some(cp) = dirs::config_dir() {
        config_path = cp;
    } else {
        println!("Cannot create config directory{:?}", config_path);
        std::process::exit(0);
    }
    let observer_ward = config_path.join("observer_ward");
    if !observer_ward.is_dir() || !observer_ward.exists() {
        std::fs::create_dir_all(&observer_ward).unwrap_or_default();
    }
    observer_ward
});

impl<'a> Helper<'a> {
    pub fn new(config: &'a ObserverWardConfig) -> Self {
        let ro = RequestOption::new(
            &config.timeout,
            &config.proxy,
            &config.verify,
            config.silent,
            config.danger,
            &config.ua,
        );
        Self {
            request_option: ro,
            config_path: &OBSERVER_WARD_PATH,
            config,
            msg: Default::default(),
        }
    }
    fn update_fingerprint(&mut self) {
        let fingerprint_path = self.config_path.join("web_fingerprint_v3.json");
        self.download_file_from_github(
            "https://0x727.github.io/FingerprintHub/web_fingerprint_v3.json",
            fingerprint_path
                .to_str()
                .unwrap_or("web_fingerprint_v3.json"),
        );
        self.download_file_from_github(
            "https://0x727.github.io/FingerprintHub/plugins/tags.yaml",
            self.config_path
                .join("tags.yaml")
                .to_str()
                .unwrap_or("tags.yaml"),
        );
    }
    fn update_plugins(&mut self) {
        let plugins_zip_path = self.config_path.join("plugins.zip");
        let extract_target_path = self.config_path;
        self.download_file_from_github(
            "https://github.com/0x727/FingerprintHub/releases/download/default/plugins.zip",
            plugins_zip_path.to_str().unwrap_or("plugins.zip"),
        );
        match extract_plugins_zip(&plugins_zip_path, extract_target_path) {
            Ok(_) => {
                println!("It has been extracted to the {:?}", extract_target_path);
            }
            Err(err) => {
                println!("{:?}", err);
                println!("Please manually unzip the plugins to the directory");
            }
        }
    }
    pub fn run(&mut self) -> HashMap<String, String> {
        if self.config.update_fingerprint {
            self.update_fingerprint();
        }
        if self.config.update_self {
            self.update_self();
        }
        if self.config.update_plugins {
            self.update_plugins();
        }
        if !self.msg.is_empty() {
            for (k, v) in &self.msg {
                println!("{}:{}", k, v);
            }
        }
        self.msg.clone()
    }
}

impl<'a> Helper<'_> {
    pub fn update_self(&mut self) {
        // https://doc.rust-lang.org/reference/conditional-compilation.html
        let mut base_url =
            String::from("https://github.com/0x727/ObserverWard/releases/download/default/");
        let mut download_name = "observer_ward_amd64";
        if cfg!(target_os = "windows") {
            download_name = "observer_ward.exe";
        } else if cfg!(target_os = "linux") {
            download_name = "observer_ward_amd64";
        } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
            download_name = "observer_ward_darwin";
        } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
            download_name = "observer_ward_aarch64_darwin";
        };
        base_url.push_str(download_name);
        let save_filename = "update_".to_owned() + download_name;
        self.download_file_from_github(&base_url, &save_filename);
        println!(
            "Please rename the file {} => {}",
            save_filename, download_name
        );
    }

    pub fn read_nmap_fingerprint(&mut self) -> Vec<NmapFingerPrint> {
        let nmap_fingerprint_path = self.config_path.join("nmap_service_probes.json");
        if let Ok(mut file) = File::open(nmap_fingerprint_path) {
            let mut data = String::new();
            file.read_to_string(&mut data).ok();
            let nmap_fingerprint: Vec<NmapFingerPrint> =
                serde_json::from_str(&data).expect("BAD JSON");
            return nmap_fingerprint;
        } else {
            println!("The nmap fingerprint library cannot be found in the current directory!");
        }
        Vec::new()
    }
    fn yaml_to_finger(&self, yaml_path: &Path) -> Vec<WebFingerPrint> {
        let mut web_fingerprint: Vec<WebFingerPrint> = vec![];
        if let Ok(file) = File::open(yaml_path) {
            match serde_yaml::from_reader::<_, VerifyWebFingerPrint>(&file) {
                Ok(verify_fingerprints) => {
                    for mut verify_fingerprint in verify_fingerprints.fingerprint {
                        verify_fingerprint.name = verify_fingerprints.name.clone();
                        verify_fingerprint.priority = verify_fingerprints.priority;
                        web_fingerprint.push(verify_fingerprint);
                    }
                }
                Err(err) => {
                    println!("{}", err);
                    std::process::exit(0);
                }
            };
        }
        web_fingerprint
    }
    pub fn read_web_fingerprint(&mut self, config: &ObserverWardConfig) -> Vec<WebFingerPrint> {
        if let Some(verify_path) = &config.verify {
            let verify_file = PathBuf::from(verify_path);
            if verify_file.exists() {
                return self.yaml_to_finger(&verify_file);
            }
        }
        if let Some(yaml_path) = &config.yaml {
            let walker = std::fs::read_dir(yaml_path).into_iter();
            let mut web_fingerprint: Vec<WebFingerPrint> = vec![];
            for entry in walker.flatten().filter_map(|e| e.ok()).filter(is_hidden) {
                if entry.path().extension() == Some("yaml".as_ref()) {
                    web_fingerprint.extend(self.yaml_to_finger(&entry.path()));
                }
            }
            if !config.silent {
                println!("Load {} fingerprints.", web_fingerprint.len());
            }
            if let Some(json_path) = &config.gen {
                let out = File::create(json_path).expect("Failed to create file");
                serde_json::to_writer(out, &web_fingerprint).expect("Failed to generate json file");
                println!(
                    "completed generating json format files, totaling {} items",
                    web_fingerprint.len()
                );
            }
            return web_fingerprint;
        }
        let mut web_fingerprint_path = PathBuf::from("web_fingerprint_v3.json");
        // 如果有指定路径的指纹库
        if let Some(p) = &config.fpath {
            web_fingerprint_path = PathBuf::from(p);
            if !web_fingerprint_path.exists() {
                println!("The specified fingerprint path does not exist");
                std::process::exit(1);
            }
        } else {
            // 如果当前运行目录下没有指纹库，把路径改为config目录下的
            if !web_fingerprint_path.exists() {
                web_fingerprint_path = self.config_path.join("web_fingerprint_v3.json");
            }
            if !web_fingerprint_path.exists() {
                self.update_fingerprint();
            }
        }
        if let Ok(file) = File::open(web_fingerprint_path) {
            if let Ok(web_fingerprint) = serde_json::from_reader::<_, Vec<WebFingerPrint>>(&file) {
                return web_fingerprint;
            } else {
                println!("The fingerprint format is incorrect. Please update the fingerprint library again");
            };
        } else {
            println!("The fingerprint library cannot be found in the current directory!");
            println!("Update fingerprint library with `-u` parameter!");
        }
        std::process::exit(1);
    }

    pub fn read_results_file(&self) -> Vec<WhatWebResult> {
        let mut results: Vec<WhatWebResult> = Vec::new();
        let read_file_data = |path: &str| {
            let mut file = match File::open(path) {
                Err(err) => {
                    println!("{}", err);
                    std::process::exit(0);
                }
                Ok(file) => file,
            };
            let mut data = String::new();
            file.read_to_string(&mut data).ok();
            data
        };
        if let Some(json_path) = &self.config.json {
            let data = read_file_data(json_path);
            let wwr: Vec<WhatWebResult> = serde_json::from_str(&data).expect("BAD JSON");
            results.extend(wwr);
        }
        if let Some(csv_path) = &self.config.csv {
            let rdr = Reader::from_path(csv_path).expect("BAD CSV");
            let iter: csv::DeserializeRecordsIntoIter<File, WhatWebResult> = rdr.into_deserialize();
            let wwr: Vec<WhatWebResult> = iter.filter_map(Result::ok).collect();
            results.extend(wwr);
        }
        results
    }
    fn download_file_from_github(&mut self, update_url: &'a str, filename: &'a str) {
        let proxy = self.request_option.proxy.as_ref().cloned();
        let proxy_obj = Proxy::custom(move |_url| proxy.clone());
        let client = reqwest::blocking::Client::builder().proxy(proxy_obj);
        if let Ok(downloading_client) = client.build() {
            if let Ok(response) = downloading_client.get(update_url).send() {
                let mut file = File::create(filename).unwrap();
                let mut content = Cursor::new(response.bytes().unwrap_or_default());
                std::io::copy(&mut content, &mut file).unwrap_or_default();
                self.msg.insert(
                    String::from(update_url),
                    format!(
                        "=> {}' file size => {:?}",
                        filename,
                        file.metadata().unwrap().len()
                    ),
                );
                return;
            }
        }
        self.msg.insert(
            String::from("err"),
            format!(
                "Update failed, please download {} to local directory manually.",
                update_url
            ),
        );
    }
}

fn is_hidden(entry: &std::fs::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| !s.starts_with('.'))
        .unwrap_or(false)
}

pub fn read_file_to_target(file_path: &str) -> HashSet<String> {
    if let Ok(lines) = read_lines(file_path) {
        let target_list: Vec<String> = lines.map_while(Result::ok).collect();
        return HashSet::from_iter(target_list);
    }
    HashSet::from_iter([])
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub fn print_results_and_save(results: Vec<WhatWebResult>, config: &ObserverWardConfig) {
    if let Some(json_path) = &config.json {
        let out = File::create(json_path).expect("Failed to create file");
        serde_json::to_writer(out, &results).expect("Failed to save file")
    }
    let mut table = Table::new();
    let mut headers = vec![
        Cell::new("url"),
        Cell::new("name"),
        Cell::new("length"),
        Cell::new("status_code"),
        Cell::new("title"),
        Cell::new("priority"),
    ];
    if config.use_nuclei() {
        headers.push(Cell::new("plugins"))
    }
    table.set_titles(Row::new(headers.clone()));
    for res in &results {
        let wwn: Vec<String> = res.name.iter().map(String::from).collect();
        let status_code = reqwest::StatusCode::from_u16(res.status_code).unwrap_or_default();
        let mut status_code_color = Attr::ForegroundColor(color::RED);
        if status_code.is_success() {
            status_code_color = Attr::ForegroundColor(color::GREEN);
        }
        let mut rows = vec![
            Cell::new(res.url.as_str()),
            Cell::new(&wwn.join("\n")).with_style(Attr::ForegroundColor(color::GREEN)),
            Cell::new(&res.length.to_string()),
            Cell::new(&res.status_code.to_string()).with_style(status_code_color),
            Cell::new(&textwrap::fill(res.title.as_str(), 40)),
            Cell::new(&res.priority.to_string()),
        ];
        if config.use_nuclei() {
            let wp: Vec<String> = res.plugins.iter().map(String::from).collect();
            rows.push(Cell::new(&wp.join("\n")).with_style(Attr::ForegroundColor(color::RED)))
        }
        table.add_row(Row::new(rows));
    }
    if let Some(csv_path) = &config.csv {
        let out = File::create(csv_path).expect("Failed to create file");
        table.to_csv(out).expect("Failed to save file");
    }
    let mut table = Table::new();
    table.set_titles(Row::new(headers.clone()));
    for res in &results {
        if config.filter && res.name.is_empty() {
            continue;
        }
        let wwn: Vec<String> = res.name.iter().map(String::from).collect();
        let status_code = reqwest::StatusCode::from_u16(res.status_code).unwrap_or_default();
        let mut status_code_color = Attr::ForegroundColor(color::RED);
        if status_code.is_success() {
            status_code_color = Attr::ForegroundColor(color::GREEN);
        }
        let mut rows = vec![
            Cell::new(res.url.as_str()),
            Cell::new(&wwn.join("\n")).with_style(Attr::ForegroundColor(color::GREEN)),
            Cell::new(&res.length.to_string()),
            Cell::new(&res.status_code.to_string()).with_style(status_code_color),
            Cell::new(&textwrap::fill(res.title.as_str(), 40)),
            Cell::new(&res.priority.to_string()),
        ];
        if config.use_nuclei() {
            let wp: Vec<String> = res.plugins.iter().map(String::from).collect();
            rows.push(Cell::new(&wp.join("\n")).with_style(Attr::ForegroundColor(color::RED)))
        }
        table.add_row(Row::new(rows));
    }
    if !table.is_empty() && !config.silent {
        println!("{}", "Important technology:".yellow());
        table.printstd();
    }
}

fn extract_plugins_zip(f_name: &Path, extract_target_path: &Path) -> Result<(), Error> {
    let plugins_path = extract_target_path.join("plugins");
    if plugins_path.exists() {
        std::fs::remove_dir_all(plugins_path)?;
    }
    let zipfile = File::open(f_name)?;
    let mut archive = zip::ZipArchive::new(zipfile)?;
    archive.extract(extract_target_path)?;
    Ok(())
}

static NUCLEI_TAGS: Lazy<HashMap<String, Vec<Vec<String>>>> =
    Lazy::new(|| -> HashMap<String, Vec<Vec<String>>> {
        let mut config_path = PathBuf::new();
        if let Some(cp) = dirs::config_dir() {
            config_path = cp;
        } else {
            println!("Cannot create config directory{:?}", config_path);
            std::process::exit(0);
        }
        let observer_ward = config_path.join("observer_ward");
        if !observer_ward.is_dir() || !observer_ward.exists() {
            std::fs::create_dir_all(&observer_ward).unwrap_or_default();
        }
        let tags_path = observer_ward.join("tags.yaml");
        if !tags_path.exists() {
            println!(
                "Unable to find the {:?} file, please update with the `-u` parameter",
                tags_path
            );
            std::process::exit(0);
        }
        let mut tags_map: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        // 读入tags.yaml文件，解析
        if let Ok(file) = File::open(tags_path) {
            match serde_yaml::from_reader::<_, HashMap<String, Vec<Vec<String>>>>(&file) {
                Ok(s) => tags_map = s,
                Err(err) => {
                    println!("tags.yaml serde err: {}", err);
                    std::process::exit(0);
                }
            };
            return tags_map;
        } else {
            println!("The tags.yaml file cannot be found in the current directory!");
        }
        tags_map
    });

pub async fn get_plugins_by_nuclei(
    mut wwr: WhatWebResult,
    config: &ObserverWardConfig,
) -> WhatWebResult {
    let mut plugins_set: HashSet<String> = HashSet::new();
    let mut exist_plugins: Vec<String> = Vec::new();
    let use_tags = config.path.is_some();
    let mut template_condition = Vec::new();
    for name in wwr.name.iter() {
        if let Some(plugins_path) = &config.plugins {
            let plugins_name_path = Path::new(plugins_path).join(name);
            if plugins_name_path.exists() {
                if let Some(p_path) = plugins_name_path.to_str() {
                    exist_plugins.push(p_path.to_string())
                }
            }
        }
        if use_tags {
            if let Some(ts) = NUCLEI_TAGS.get(name) {
                template_condition.push(gen_nuclei_tags(ts));
            }
        }
    }
    if let Some(t) = &config.path {
        exist_plugins.push(t.to_string());
    }
    if exist_plugins.is_empty() || template_condition.is_empty() {
        return wwr;
    }
    let mut command_line = Command::new("nuclei");
    command_line.args([
        "-u",
        &wwr.url,
        "-no-color",
        "-timeout",
        &(config.timeout + 5).to_string(),
        "-es",
        "info", //排除info模板
    ]);
    if let Some(nargs) = &config.nargs {
        let args: Vec<&str> = nargs.split(' ').collect();
        for arg in args {
            command_line.arg(arg);
        }
    }
    for p in exist_plugins.iter() {
        command_line.args(["-t", p]);
    }
    if !template_condition.is_empty() {
        command_line.args(["-tc", &template_condition.join("||")]);
    }
    command_line.args(["-silent", "-jsonl", "-duc"]);
    if config.irr {
        command_line.args(["-irr"]);
    }
    if let Ok(err) = command_line
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        if !config.silent {
            println!("{}", String::from_utf8_lossy(&err.stderr))
        }
    }
    let output = command_line.output().await.expect("command_line_output");
    if let Ok(template_output) = String::from_utf8(output.stdout) {
        let templates_output: Vec<String> = template_output
            .split_terminator('\n')
            .map(String::from)
            .collect();
        for line in templates_output.iter() {
            let template: TemplateResult = serde_json::from_str(line).unwrap_or_default();

            if !config.silent {
                print!("[{}] ", template.info.severity.to_string().green());
                print!("[{}] ", template.template_id.to_string().red());
                println!("| [{}] ", template.matched_at);
                if !template.curl_command.is_empty() {
                    let patch_curl_command = format!("{} --path-as-is -k", template.curl_command);
                    println!("{}", patch_curl_command.dark_blue());
                }
            }
            if config.irr {
                if let Some(mut t) = wwr.plugins_result {
                    t.push(template.clone());
                    wwr.plugins_result = Some(t);
                } else {
                    wwr.plugins_result = Some(vec![template.clone()]);
                }
            }
            plugins_set.insert(template.template_id);
        }
    }
    wwr.plugins = plugins_set;
    if !wwr.plugins.is_empty() {
        wwr.priority += 1;
    }
    wwr
}

fn gen_nuclei_tags(tags_list: &Vec<Vec<String>>) -> String {
    let mut or_condition = Vec::new();
    for tags in tags_list {
        // 只留单个的tags，防止误报
        if tags.len() == 1 {
            or_condition.push(format!("contains(tags,'{}')", tags[0]))
        } else {
            let mut and_condition = Vec::new();
            for tag in tags {
                and_condition.push(format!("contains(tags,'{}')", tag));
            }
            or_condition.push(format!("({})", and_condition.join("&&")));
        }
    }
    or_condition.join("||")
}

#[derive(Clone)]
pub struct ObserverWard {
    what_server_ins: WhatServer,
    what_web_ins: WhatWeb,
    config: ObserverWardConfig,
}

impl Default for ObserverWard {
    fn default() -> Self {
        let config = ObserverWardConfig::new();
        let mut helper = Helper::new(&config);
        let web_fingerprint = helper.read_web_fingerprint(&config);
        let mut nmap_fingerprint = vec![];
        if config.service {
            nmap_fingerprint = helper.read_nmap_fingerprint();
        }
        ObserverWard::new(config, web_fingerprint, nmap_fingerprint)
    }
}

impl ObserverWard {
    pub fn new(
        config: ObserverWardConfig,
        web_fingerprint: Vec<WebFingerPrint>,
        nmap_fingerprint: Vec<NmapFingerPrint>,
    ) -> Self {
        let request_option = RequestOption::new(
            &config.timeout,
            &config.proxy,
            &config.verify,
            config.silent,
            config.danger,
            &config.ua,
        );
        let what_server_ins = WhatServer::new(300, nmap_fingerprint);
        let what_web_ins = WhatWeb::new(request_option, web_fingerprint);
        Self {
            what_server_ins,
            what_web_ins,
            config,
        }
    }
    pub async fn scan(&self, targets: HashSet<String>) -> Vec<WhatWebResult> {
        let config = self.config.clone();
        let what_web_ins = self.what_web_ins.clone();
        let what_server_ins = self.what_server_ins.clone();
        let (what_web_sender, mut what_web_receiver) = unbounded();
        let (mut what_server_sender, mut what_server_receiver) = unbounded();
        let (mut verify_sender, mut verify_receiver) = unbounded();
        let (mut results_sender, mut results_receiver) = unbounded();
        let mut vec_results: Vec<WhatWebResult> = vec![];
        let config_thread = config.thread;
        let webhook = config.webhook.clone();
        let webhook_auth = config.webhook_auth.clone();
        let what_web_handle = tokio::task::spawn(async move {
            let mut worker = FuturesUnordered::new();
            let mut targets_iter = targets.iter();
            for _ in 0..config_thread {
                match targets_iter.next() {
                    Some(target) => worker.push(what_web_ins.scan(target.to_string())),
                    None => {
                        break;
                    }
                }
            }
            while let Some(result) = worker.next().await {
                if let Some(target) = targets_iter.next() {
                    worker.push(what_web_ins.scan(target.to_string()));
                }
                what_web_sender.unbounded_send(result).unwrap_or_default();
            }
            true
        });
        let what_server_handle = tokio::task::spawn(async move {
            let mut worker = FuturesUnordered::new();
            for _ in 0..3 {
                match what_web_receiver.next().await {
                    Some(w) => worker.push(what_server_ins.scan(w)),
                    None => {
                        break;
                    }
                }
            }
            while let Some(wwr) = worker.next().await {
                if let Some(v_wwr) = what_web_receiver.next().await {
                    worker.push(what_server_ins.scan(v_wwr));
                }
                if !config.silent {
                    print_what_web(&wwr);
                }
                what_server_sender.start_send(wwr).unwrap_or_default();
            }
            true
        });
        let verify_handle = tokio::task::spawn(async move {
            if config.use_nuclei() {
                let mut worker = FuturesUnordered::new();
                for _ in 0..3 {
                    match what_server_receiver.next().await {
                        Some(w) => {
                            worker.push(get_plugins_by_nuclei(w, &config));
                        }
                        None => {
                            break;
                        }
                    }
                }
                while let Some(wwr) = worker.next().await {
                    if let Some(v_wwr) = what_server_receiver.next().await {
                        worker.push(get_plugins_by_nuclei(v_wwr, &config));
                    }
                    verify_sender.start_send(wwr).unwrap_or_default();
                }
            } else {
                while let Some(wwr) = what_server_receiver.next().await {
                    verify_sender.start_send(wwr).unwrap_or_default();
                }
            }
            true
        });
        let results_handle = tokio::task::spawn(async move {
            if let Some(webhook_url) = webhook {
                let mut worker = FuturesUnordered::new();
                for _ in 0..3 {
                    match verify_receiver.next().await {
                        Some(w) => {
                            worker.push(webhook_results(w, &webhook_url, &webhook_auth));
                        }
                        None => {
                            break;
                        }
                    }
                }
                while let Some(wwr) = worker.next().await {
                    if let Some(w) = verify_receiver.next().await {
                        worker.push(webhook_results(w, &webhook_url, &webhook_auth));
                    }
                    results_sender.start_send(wwr).unwrap_or_default();
                }
            } else {
                while let Some(wwr) = verify_receiver.next().await {
                    results_sender.start_send(wwr).unwrap_or_default();
                }
            }
            true
        });
        let (_r1, _r2, _r3, _r4) = tokio::join!(
            what_web_handle,
            what_server_handle,
            verify_handle,
            results_handle
        );
        while let Some(wwr) = results_receiver.next().await {
            vec_results.push(wwr);
        }
        if vec_results.len() < 2000 {
            vec_results.sort_by(|a, b| b.priority.cmp(&a.priority));
        }
        vec_results
    }
    pub fn reload(&mut self, config: &ObserverWardConfig) {
        let mut helper = Helper::new(config);
        let web_fingerprint = helper.read_web_fingerprint(config);
        let mut nmap_fingerprint = vec![];
        if config.service {
            nmap_fingerprint = helper.read_nmap_fingerprint();
        }
        let request_option = RequestOption::new(
            &config.timeout,
            &config.proxy,
            &config.verify,
            config.silent,
            config.danger,
            &config.ua,
        );
        let what_server_ins = WhatServer::new(300, nmap_fingerprint);
        let what_web_ins = WhatWeb::new(request_option, web_fingerprint);
        self.config = config.clone();
        self.what_web_ins = what_web_ins;
        self.what_server_ins = what_server_ins;
    }
}

// 去重
pub fn strings_to_urls(domains: String) -> HashSet<String> {
    let target_list = domains
        .split_terminator('\n')
        .map(String::from)
        .collect::<Vec<_>>();
    HashSet::from_iter(target_list)
}
