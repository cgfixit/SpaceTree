//! Extension identity, scan-root share, and the legend. No egui.

use std::collections::BTreeMap;

use crate::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Clone, Debug)]
pub struct LegendRow {
    pub key: String,
    pub physical: u64,
    pub files: u64,
}

const NAMED: &[(&str, Rgb, &str)] = &[
    (
        "jpg",
        Rgb {
            r: 220,
            g: 60,
            b: 160,
        },
        "JPEG Image",
    ),
    (
        "png",
        Rgb {
            r: 230,
            g: 120,
            b: 180,
        },
        "PNG Image",
    ),
    (
        "mp4",
        Rgb {
            r: 220,
            g: 60,
            b: 60,
        },
        "MP4 Video",
    ),
    (
        "wav",
        Rgb {
            r: 70,
            g: 110,
            b: 230,
        },
        "WAV Audio",
    ),
    (
        "zip",
        Rgb {
            r: 180,
            g: 200,
            b: 40,
        },
        "Zip Archive",
    ),
    (
        "gz",
        Rgb {
            r: 160,
            g: 190,
            b: 50,
        },
        "Gzip Archive",
    ),
    (
        "7z",
        Rgb {
            r: 180,
            g: 60,
            b: 200,
        },
        "7-Zip Archive",
    ),
    (
        "pdf",
        Rgb {
            r: 210,
            g: 70,
            b: 50,
        },
        "PDF Document",
    ),
    (
        "csv",
        Rgb {
            r: 230,
            g: 190,
            b: 40,
        },
        "Comma-Separated Values",
    ),
    (
        "xlsx",
        Rgb {
            r: 40,
            g: 180,
            b: 160,
        },
        "Excel Spreadsheet",
    ),
    (
        "vhdx",
        Rgb {
            r: 80,
            g: 80,
            b: 220,
        },
        "Virtual Disk Image",
    ),
    (
        "vhd",
        Rgb {
            r: 80,
            g: 80,
            b: 220,
        },
        "Virtual Disk Image",
    ),
    (
        "cpp",
        Rgb {
            r: 230,
            g: 50,
            b: 140,
        },
        "C++ Source",
    ),
    (
        "rs",
        Rgb {
            r: 90,
            g: 160,
            b: 230,
        },
        "Rust Source",
    ),
];

const FALLBACK: [Rgb; 12] = [
    Rgb {
        r: 66,
        g: 133,
        b: 244,
    },
    Rgb {
        r: 52,
        g: 168,
        b: 83,
    },
    Rgb {
        r: 251,
        g: 188,
        b: 5,
    },
    Rgb {
        r: 234,
        g: 67,
        b: 53,
    },
    Rgb {
        r: 154,
        g: 160,
        b: 166,
    },
    Rgb {
        r: 255,
        g: 112,
        b: 67,
    },
    Rgb {
        r: 0,
        g: 172,
        b: 193,
    },
    Rgb {
        r: 156,
        g: 39,
        b: 176,
    },
    Rgb {
        r: 121,
        g: 85,
        b: 72,
    },
    Rgb {
        r: 63,
        g: 81,
        b: 181,
    },
    Rgb {
        r: 0,
        g: 150,
        b: 136,
    },
    Rgb {
        r: 255,
        g: 152,
        b: 0,
    },
];

pub fn ext_key(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let Some((stem, ext)) = base.rsplit_once('.') else {
        return String::new();
    };
    if stem.is_empty() || ext.is_empty() {
        return String::new();
    }
    match ext.to_ascii_lowercase().as_str() {
        "jpeg" => "jpg".to_string(),
        "tiff" => "tif".to_string(),
        "yml" => "yaml".to_string(),
        "htm" => "html".to_string(),
        other => other.to_string(),
    }
}

pub fn ext_label(key: &str) -> String {
    if key.is_empty() {
        "(none)".to_string()
    } else {
        format!(".{key}")
    }
}

pub fn ext_description(key: &str) -> String {
    if key.is_empty() {
        return "No Extension".to_string();
    }
    if let Some((_, _, desc)) = NAMED.iter().find(|(k, _, _)| *k == key) {
        return (*desc).to_string();
    }
    format!("{} File", key.to_ascii_uppercase())
}

pub fn ext_color(key: &str) -> Rgb {
    if key.is_empty() {
        return Rgb {
            r: 168,
            g: 168,
            b: 168,
        };
    }
    if let Some((_, rgb, _)) = NAMED.iter().find(|(k, _, _)| *k == key) {
        return *rgb;
    }
    FALLBACK[(fnv1a(key) % FALLBACK.len() as u64) as usize]
}

pub fn format_scan_share(size: u64, root_size: u64) -> String {
    if root_size == 0 {
        return "0.00%".to_string();
    }
    let hundredths =
        (u128::from(size) * 10_000 + u128::from(root_size) / 2) / u128::from(root_size);
    format!("{}.{:02}%", hundredths / 100, hundredths % 100)
}

pub fn share_px(size: u64, root_size: u64, width: u32) -> u32 {
    if root_size == 0 || width == 0 || size == 0 {
        return 0;
    }
    let v = u128::from(size) * u128::from(width) / u128::from(root_size);
    v.min(u128::from(width)) as u32
}

pub fn legend_of(root: &Node) -> Vec<LegendRow> {
    let mut map: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    fn walk(node: &Node, map: &mut BTreeMap<String, (u64, u64)>) {
        if node.is_dir {
            for child in &node.children {
                walk(child, map);
            }
            return;
        }
        let entry = map.entry(node.color_ext.clone()).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(node.size);
        entry.1 = entry.1.saturating_add(node.files);
    }
    walk(root, &mut map);
    let mut rows: Vec<LegendRow> = map
        .into_iter()
        .map(|(key, (physical, files))| LegendRow {
            key,
            physical,
            files,
        })
        .collect();
    rows.sort_by(|a, b| b.physical.cmp(&a.physical).then_with(|| a.key.cmp(&b.key)));
    rows
}

fn fnv1a(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
