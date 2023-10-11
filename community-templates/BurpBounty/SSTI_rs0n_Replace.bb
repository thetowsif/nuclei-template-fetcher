[
  {
    "ProfileName": "SSTI_rs0n_Replace",
    "Name": "",
    "Enabled": true,
    "Scanner": 1,
    "Author": "rs0n",
    "Payloads": [
      "true,{{349*349}}",
      "true,${349*349}",
      "true,\u003c%\u003d 349*349 %\u003e",
      "true,${{349*349}}",
      "true,#{349*349}"
    ],
    "Encoder": [],
    "UrlEncode": false,
    "CharsToUrlEncode": "",
    "Grep": [
      "true,,121801"
    ],
    "Tags": [
      "All"
    ],
    "PayloadResponse": false,
    "NotResponse": false,
    "TimeOut1": "",
    "TimeOut2": "",
    "isTime": false,
    "contentLength": "",
    "iscontentLength": false,
    "CaseSensitive": false,
    "ExcludeHTTP": false,
    "OnlyHTTP": false,
    "IsContentType": false,
    "ContentType": "",
    "HttpResponseCode": "",
    "NegativeCT": false,
    "IsResponseCode": false,
    "ResponseCode": "",
    "NegativeRC": false,
    "urlextension": "",
    "isurlextension": false,
    "NegativeUrlExtension": false,
    "MatchType": 1,
    "Scope": 0,
    "RedirType": 0,
    "MaxRedir": 0,
    "payloadPosition": 1,
    "payloadsFile": "",
    "grepsFile": "",
    "IssueName": "Server-Side Template Injection (SSTI)",
    "IssueSeverity": "High",
    "IssueConfidence": "Firm",
    "IssueDetail": "A server-side template injection occurs when an attacker is able to use native template syntax to inject a malicious payload into a template, which is then executed server-side.\n\nTemplate engines are designed to generate web pages by combining fixed templates with volatile data. Server-side template injection attacks can occur when user input is concatenated directly into a template, rather than passed in as data. This allows attackers to inject arbitrary template directives in order to manipulate the template engine, often enabling them to take complete control of the server.\n\nhttps://book.hacktricks.xyz/pentesting-web/ssti-server-side-template-injection",
    "RemediationDetail": "",
    "IssueBackground": "",
    "RemediationBackground": "",
    "Header": [],
    "VariationAttributes": [],
    "InsertionPointType": [
      18,
      65,
      32,
      36,
      7,
      1,
      2,
      6,
      33,
      5,
      35,
      34,
      64,
      0,
      3,
      4,
      37,
      127,
      65,
      32,
      36,
      7,
      1,
      2,
      6,
      33,
      5,
      35,
      34,
      64,
      0,
      3,
      4,
      37,
      127
    ],
    "Scanas": false,
    "Scantype": 0,
    "pathDiscovery": false
  }
]