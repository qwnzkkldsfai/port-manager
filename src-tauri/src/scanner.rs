use encoding_rs_io::DecodeReaderBytesBuilder;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const CONFIG_EXTS: &[&str] = &[
    "ini", "conf", "cfg", "yaml", "yml", "json", "toml", "xml", "properties", "env", "config",
    "txt", "rc",
];

const SPECIAL_NAMES: &[&str] = &[
    ".env",
    "dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "nginx.conf",
    "redis.conf",
    "my.ini",
    "my.cnf",
    "httpd.conf",
    "server.properties",
    "application.properties",
    "application.yml",
    "application.yaml",
];

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// Pattern 1: explicit keyword + separator + port.
/// kw matches: bare "port"/"ports", any `<word>(._-)port`, or `listen[...]`/`bind[...]` variants.
/// To avoid matching the "port" inside "transport"/"support", the kw must follow a non-word char or start.
/// An optional IP prefix `1.2.3.4:` is consumed before the captured port, so `port: 0.0.0.0:8080` → 8080.
static KW_PORT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?ix)
        (?:^|[^a-z0-9_])
        (?P<kw>
            (?:[a-z][\w]*[._-])? ports?
          | http s? [._-]? ports?
          | tcp [._-]? ports?
          | udp [._-]? ports?
          | listen [._-]? port s?
          | admin [._-]? port s?
          | metrics [._-]? port s?
          | api [._-]? port s?
          | grpc [._-]? port s?
          | jmx [._-]? port s?
          | db [._-]? port s?
          | listenport s?
          | httpsport | httpport | adminport | metricsport
          | listen (?: s? | s_on | _on | port s? | _port s? | _address | address )
          | bind (?: _? address | _? port s? )?
        )
        (?:[^\w]|$)
        \s* [:=]? \s*
        ["']?
        (?: (?:\d{1,3}\.){3}\d{1,3} : )?
        (?P<num>[1-9]\d{1,4})
        (?P<after>[^\d.]|$)
        "#,
    )
    .unwrap()
});

/// Pattern 2: URL or IP:port (e.g. `http://localhost:8080`, `127.0.0.1:5432`).
/// The host part is consumed before the captured port, so we don't grab IP octets by mistake.
static URL_PORT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?ix)
        (?P<host>
            https? :// [a-z0-9._\-]+
          | \b localhost
          | \b \d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}
        )
        :
        (?P<num>[1-9]\d{1,4})
        (?P<after>[^\d.]|$)
        "#,
    )
    .unwrap()
});

/// Pattern 3: CLI flags like `--port 8080` or `--listen=:8080`.
static CLI_PORT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?ix)
        (?P<flag>--(?:port|listen|bind|http[-_]?port|https[-_]?port|metrics[-_]?port|admin[-_]?port|api[-_]?port|grpc[-_]?port))
        [=\s] \s*
        ["']?
        (?: (?:\d{1,3}\.){3}\d{1,3} : )?
        (?P<num>[1-9]\d{1,4})
        (?P<after>[^\d.]|$)
        "#,
    )
    .unwrap()
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCandidate {
    pub port: u16,
    pub file: String,
    pub line: u32,
    pub context: String,
    pub keyword: String,
    #[serde(default)]
    pub occurrences: u32,
}

fn is_config_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if CONFIG_EXTS.contains(&lower.as_str()) {
            return true;
        }
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_ascii_lowercase();
        if SPECIAL_NAMES.iter().any(|s| s == &lower) {
            return true;
        }
    }
    false
}

fn read_text(path: &Path) -> std::io::Result<String> {
    let f = File::open(path)?;
    let meta = f.metadata()?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file too large",
        ));
    }
    let mut decoded = String::new();
    let mut reader = DecodeReaderBytesBuilder::new().build(f);
    reader.read_to_string(&mut decoded)?;
    Ok(decoded)
}

fn read_text_lossy(path: &Path) -> std::io::Result<String> {
    match read_text(path) {
        Ok(s) => Ok(s),
        Err(_) => {
            let mut f = File::open(path)?;
            let meta = f.metadata()?;
            if meta.len() > MAX_FILE_SIZE {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "file too large",
                ));
            }
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            let (cow, _, _) = encoding_rs::GBK.decode(&buf);
            Ok(cow.into_owned())
        }
    }
}

fn extract_from_line(line: &str, raw: &mut Vec<(u16, String)>) {
    // Skip lines that look like noise (very long, or pure binary-ish).
    if line.len() > 4000 {
        return;
    }
    let lowered_line = line.trim().chars().take(180).collect::<String>();

    for cap in KW_PORT_REGEX.captures_iter(line) {
        if let Some(num_match) = cap.name("num") {
            if let Ok(n) = num_match.as_str().parse::<u32>() {
                if (10..=65535).contains(&n) {
                    let kw = cap.name("kw").map(|m| m.as_str().to_lowercase()).unwrap_or_default();
                    raw.push((n as u16, format!("[{}] {}", kw, lowered_line)));
                }
            }
        }
    }
    for cap in URL_PORT_REGEX.captures_iter(line) {
        if let Some(num_match) = cap.name("num") {
            if let Ok(n) = num_match.as_str().parse::<u32>() {
                if (10..=65535).contains(&n) {
                    raw.push((n as u16, format!("[url] {}", lowered_line)));
                }
            }
        }
    }
    for cap in CLI_PORT_REGEX.captures_iter(line) {
        if let Some(num_match) = cap.name("num") {
            if let Ok(n) = num_match.as_str().parse::<u32>() {
                if (10..=65535).contains(&n) {
                    let flag = cap.name("flag").map(|m| m.as_str().to_lowercase()).unwrap_or_default();
                    raw.push((n as u16, format!("[{}] {}", flag, lowered_line)));
                }
            }
        }
    }
}

#[derive(Default)]
struct PortAccum {
    first_file: String,
    first_line: u32,
    first_context: String,
    first_keyword: String,
    count: u32,
    files: std::collections::BTreeSet<String>,
}

fn scan_text(text: &str, path_display: &str, accum: &mut HashMap<u16, PortAccum>) {
    for (idx, line) in text.lines().enumerate() {
        let mut hits: Vec<(u16, String)> = Vec::new();
        extract_from_line(line, &mut hits);
        // Dedupe per-line same-port matches (kw + url + cli often agree).
        hits.sort_by_key(|(p, _)| *p);
        hits.dedup_by(|a, b| a.0 == b.0);

        for (port, ctx_with_tag) in hits {
            let entry = accum.entry(port).or_insert_with(|| PortAccum {
                first_file: path_display.to_string(),
                first_line: (idx + 1) as u32,
                first_context: ctx_with_tag.clone(),
                first_keyword: ctx_with_tag
                    .split(']')
                    .next()
                    .map(|s| s.trim_start_matches('[').to_string())
                    .unwrap_or_default(),
                count: 0,
                files: std::collections::BTreeSet::new(),
            });
            entry.count += 1;
            entry.files.insert(path_display.to_string());
        }
    }
}

pub fn scan_path(root: &str) -> anyhow::Result<Vec<ScanCandidate>> {
    let root_path = PathBuf::from(root);
    if !root_path.exists() {
        return Err(anyhow::anyhow!("path does not exist: {}", root));
    }

    let mut accum: HashMap<u16, PortAccum> = HashMap::new();

    if root_path.is_file() {
        if is_config_file(&root_path) {
            if let Ok(text) = read_text_lossy(&root_path) {
                scan_text(&text, &root_path.to_string_lossy(), &mut accum);
            }
        }
    } else {
        for entry in WalkDir::new(&root_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            if !is_config_file(p) {
                continue;
            }
            if let Ok(text) = read_text_lossy(p) {
                scan_text(&text, &p.to_string_lossy(), &mut accum);
            }
        }
    }

    let mut out: Vec<ScanCandidate> = accum
        .into_iter()
        .map(|(port, a)| {
            let mut context = a.first_context.clone();
            if a.count > 1 {
                let extra_files = a.files.len().saturating_sub(1);
                if extra_files > 0 {
                    context = format!("{} [+另外 {} 处 / {} 个文件]", context, a.count - 1, extra_files + 1);
                } else {
                    context = format!("{} [+另外 {} 处]", context, a.count - 1);
                }
            }
            ScanCandidate {
                port,
                file: a.first_file,
                line: a.first_line,
                context,
                keyword: a.first_keyword,
                occurrences: a.count,
            }
        })
        .collect();

    out.sort_by_key(|c| c.port);
    Ok(out)
}
