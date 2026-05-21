use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize)]
pub struct ExcludedRange {
    pub protocol: String,
    pub start: u16,
    pub end: u16,
}

fn run_netsh(protocol: &str) -> anyhow::Result<String> {
    let output = Command::new("netsh")
        .args(["int", "ipv4", "show", "excludedportrange", protocol])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    // Output may be in CP936/GBK on Chinese Windows; try UTF-8 first then fall back.
    let stdout = output.stdout;
    if let Ok(s) = String::from_utf8(stdout.clone()) {
        return Ok(s);
    }
    let (cow, _, _) = encoding_rs::GBK.decode(&stdout);
    Ok(cow.into_owned())
}

fn parse(text: &str, protocol: &str) -> Vec<ExcludedRange> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        if let (Ok(start), Ok(end)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
            if start <= end && start > 0 {
                out.push(ExcludedRange {
                    protocol: protocol.to_string(),
                    start,
                    end,
                });
            }
        }
    }
    out
}

pub fn enumerate() -> anyhow::Result<Vec<ExcludedRange>> {
    let mut out = Vec::new();
    for proto in ["tcp", "udp"] {
        match run_netsh(proto) {
            Ok(text) => out.extend(parse(&text, &proto.to_uppercase())),
            Err(_) => {}
        }
    }
    Ok(out)
}

