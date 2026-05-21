use serde::Serialize;

use crate::declared::{DeclaredEntry, StoreState};
use crate::excluded::ExcludedRange;
use crate::listening::ListeningEntry;
use crate::static_ports;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceHit {
    Listening {
        protocol: String,
        local_addr: String,
        pid: u32,
        process_name: String,
        process_path: Option<String>,
    },
    Excluded {
        protocol: String,
        range_start: u16,
        range_end: u16,
    },
    Iana {
        name: String,
        description: String,
    },
    BuiltinTool {
        name: String,
        description: String,
    },
    Declared {
        id: u64,
        label: String,
        source_file: String,
        line: u32,
        context: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PortStatus {
    pub port: u16,
    pub hits: Vec<SourceHit>,
    pub free: bool,
}

pub struct Snapshot<'a> {
    pub listening: &'a [ListeningEntry],
    pub excluded: &'a [ExcludedRange],
    pub declared_entries: &'a [DeclaredEntry],
}

pub fn status_for_port(port: u16, snap: &Snapshot<'_>) -> PortStatus {
    let mut hits = Vec::new();

    for l in snap.listening.iter().filter(|l| l.port == port) {
        hits.push(SourceHit::Listening {
            protocol: l.protocol.clone(),
            local_addr: l.local_addr.clone(),
            pid: l.pid,
            process_name: l.process_name.clone(),
            process_path: l.process_path.clone(),
        });
    }
    for r in snap.excluded.iter().filter(|r| port >= r.start && port <= r.end) {
        hits.push(SourceHit::Excluded {
            protocol: r.protocol.clone(),
            range_start: r.start,
            range_end: r.end,
        });
    }
    if let Some(e) = static_ports::iana(port) {
        hits.push(SourceHit::Iana {
            name: e.name.clone(),
            description: e.description.clone(),
        });
    }
    if let Some(e) = static_ports::tool(port) {
        hits.push(SourceHit::BuiltinTool {
            name: e.name.clone(),
            description: e.description.clone(),
        });
    }
    for d in snap.declared_entries.iter().filter(|d| d.port == port) {
        hits.push(SourceHit::Declared {
            id: d.id,
            label: d.label.clone(),
            source_file: d.source_file.clone(),
            line: d.line,
            context: d.context.clone(),
        });
    }

    let free = hits.is_empty();
    PortStatus { port, hits, free }
}

pub fn occupied_set(
    listening: &[ListeningEntry],
    excluded: &[ExcludedRange],
    declared: &StoreState,
) -> std::collections::BTreeSet<u16> {
    use std::collections::BTreeSet;
    let mut set = BTreeSet::new();
    for l in listening {
        set.insert(l.port);
    }
    for r in excluded {
        for p in r.start..=r.end {
            set.insert(p);
        }
    }
    for p in static_ports::all_iana_ports() {
        set.insert(p);
    }
    for p in static_ports::all_tool_ports() {
        set.insert(p);
    }
    for p in declared.all_declared_ports() {
        set.insert(p);
    }
    set
}

#[derive(Debug, Clone, Serialize)]
pub struct FreeSegment {
    pub start: u16,
    pub end: u16,
    pub length: u32,
}

pub fn all_free_segments(
    occupied: &std::collections::BTreeSet<u16>,
    range_start: u16,
    range_end: u16,
    min_length: u32,
) -> Vec<FreeSegment> {
    let mut out = Vec::new();
    let mut cursor = range_start as u32;
    let end = range_end as u32;

    let occ_in_range: Vec<u32> = occupied
        .iter()
        .copied()
        .map(|x| x as u32)
        .filter(|p| *p >= cursor && *p <= end)
        .collect();

    for p in occ_in_range {
        if p > cursor {
            let seg_start = cursor;
            let seg_end = p - 1;
            let len = seg_end - seg_start + 1;
            if len >= min_length {
                out.push(FreeSegment {
                    start: seg_start as u16,
                    end: seg_end as u16,
                    length: len,
                });
            }
        }
        cursor = p + 1;
    }
    if cursor <= end {
        let seg_start = cursor;
        let seg_end = end;
        let len = seg_end - seg_start + 1;
        if len >= min_length {
            out.push(FreeSegment {
                start: seg_start as u16,
                end: seg_end as u16,
                length: len,
            });
        }
    }
    out
}
