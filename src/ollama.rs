//! Ollama model store: which model each content-addressed blob belongs to.
//!
//! Ollama keeps a model as a manifest under `models/manifests/<host>/
//! <namespace>/<model>/<tag>` that lists the digests of its layers, and keeps
//! each layer once as `models/blobs/sha256-<hex>`. The blob names say nothing
//! about the model, and one blob can serve several models. This module reads
//! the manifests so a blob can be shown with the models that use it, and so
//! blobs no manifest lists can be told apart. It only reads.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::{format_bytes, Node};

/// Manifests are a few kilobytes; anything far larger is not one.
const MAX_MANIFEST_BYTES: u64 = 4 << 20;

/// One layer a manifest lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OllamaLayer {
    /// File name under `blobs/`, such as `sha256-ab12…`.
    pub blob: String,
    /// What the layer holds: `model`, `template`, `params`, `config`, …
    pub kind: String,
}

/// One installed model: a manifest and the layers it lists.
#[derive(Clone, Debug)]
pub struct OllamaModel {
    /// Name as `ollama list` prints it, such as `llama3:latest`.
    pub name: String,
    pub manifest: PathBuf,
    pub layers: Vec<OllamaLayer>,
}

/// One use of a blob by a model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobUse {
    pub model: String,
    pub kind: String,
}

/// The manifests of one `models` directory.
#[derive(Clone, Debug, Default)]
pub struct OllamaStore {
    /// The `models` directory that holds `manifests` and `blobs`.
    pub dir: PathBuf,
    pub models: Vec<OllamaModel>,
    /// Manifest files that could not be read as a manifest.
    pub unreadable: Vec<PathBuf>,
    uses: BTreeMap<String, Vec<BlobUse>>,
}

impl OllamaStore {
    /// Read every manifest under `dir/manifests`. `None` when `dir` has no
    /// `manifests` directory, so it is not an Ollama store.
    pub fn load(dir: &Path) -> Option<Self> {
        let manifests = dir.join("manifests");
        if !manifests.is_dir() {
            return None;
        }
        let mut files = Vec::new();
        let mut unreadable = Vec::new();
        manifest_files(&manifests, 0, &mut files, &mut unreadable);
        files.sort();
        let mut store = OllamaStore {
            dir: dir.to_path_buf(),
            unreadable,
            ..Default::default()
        };
        for file in files {
            let Some(layers) = read_manifest(&file) else {
                store.unreadable.push(file);
                continue;
            };
            let name = model_name(&manifests, &file);
            for layer in &layers {
                let uses = store.uses.entry(layer.blob.clone()).or_default();
                let used = BlobUse {
                    model: name.clone(),
                    kind: layer.kind.clone(),
                };
                if !uses.contains(&used) {
                    uses.push(used);
                }
            }
            store.models.push(OllamaModel {
                name,
                manifest: file,
                layers,
            });
        }
        Some(store)
    }

    /// Models that list the blob with this file name; empty for a blob no
    /// manifest lists.
    pub fn uses(&self, blob: &str) -> &[BlobUse] {
        self.uses.get(blob).map_or(&[], Vec::as_slice)
    }
}

/// What to show for one file in a store's `blobs` directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobNote {
    /// A few words to show in place of the hash, such as
    /// `llama3:latest · model`.
    pub label: String,
    /// Lines for a tooltip.
    pub detail: Vec<String>,
}

/// An unfinished download: `sha256-<hex>-partial` and its numbered parts.
fn is_partial(blob: &str) -> bool {
    blob.contains("-partial")
}

fn blob_note(store: &OllamaStore, blob: &str) -> BlobNote {
    let uses = store.uses(blob);
    if let Some(first) = uses.first() {
        let label = if uses.len() == 1 {
            format!("{} · {}", first.model, first.kind)
        } else {
            format!("{} +{} · {}", first.model, uses.len() - 1, first.kind)
        };
        let mut detail = vec![if uses.len() == 1 {
            "Ollama blob used by 1 model:".to_string()
        } else {
            format!("Ollama blob shared by {} models:", uses.len())
        }];
        detail.extend(uses.iter().map(|u| format!("  {} ({})", u.model, u.kind)));
        if !store.unreadable.is_empty() {
            detail.push("Inventory incomplete; additional models may reference this blob.".into());
        } else if uses.len() > 1 {
            detail.push("It stays until every one of them is removed".to_string());
        } else if let Some(command) = remove_command(&first.model) {
            detail.push(format!("`{command}` removes it"));
        }
        BlobNote { label, detail }
    } else if !store.unreadable.is_empty() && !is_partial(blob) {
        BlobNote {
            label: "Ollama · ownership unknown".into(),
            detail: vec!["Some manifests could not be read.".into()],
        }
    } else if is_partial(blob) {
        BlobNote {
            label: "Ollama · unfinished download".into(),
            detail: vec![
                "Part of an Ollama download that did not finish.".into(),
                "Pulling the same model again resumes it; Ollama clears leftovers when its server starts."
                    .into(),
            ],
        }
    } else {
        BlobNote {
            label: "Ollama · no model uses this".into(),
            detail: vec![
                "No installed Ollama model lists this blob.".into(),
                "Ollama removes such blobs when its server starts, unless OLLAMA_NOPRUNE is set."
                    .into(),
            ],
        }
    }
}

/// Notes for every blob file of every Ollama store inside a scanned tree,
/// keyed by the file's path in the tree.
pub fn blob_notes(root: &Node) -> BTreeMap<PathBuf, BlobNote> {
    let mut out = BTreeMap::new();
    // A scan of the blobs folder itself has its manifests beside the root.
    if root.is_dir && root.name == "blobs" {
        if let Some(store) = root.path.parent().and_then(OllamaStore::load) {
            note_blobs(&store, root, &mut out);
        }
    }
    collect_notes(root, &mut out);
    out
}

fn note_blobs(store: &OllamaStore, blobs: &Node, out: &mut BTreeMap<PathBuf, BlobNote>) {
    for blob in blobs.children.iter().filter(|c| !c.is_dir) {
        if blob.name.starts_with("sha256-") {
            out.insert(blob.path.clone(), blob_note(store, &blob.name));
        }
    }
}

/// `ollama rm <model>`, or `None` for a name that is not safe to paste into
/// a shell. Names come from directory names, which can hold anything.
fn remove_command(model: &str) -> Option<String> {
    let plain = !model.is_empty()
        && !model.starts_with('-')
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/:-".contains(&b));
    plain.then(|| format!("ollama rm {model}"))
}

fn collect_notes(node: &Node, out: &mut BTreeMap<PathBuf, BlobNote>) {
    if !node.is_dir {
        return;
    }
    let child = |name: &str| node.children.iter().find(|c| c.is_dir && c.name == name);
    if let (Some(blobs), Some(_)) = (child("blobs"), child("manifests")) {
        if let Some(store) = OllamaStore::load(&node.path) {
            note_blobs(&store, blobs, out);
        }
    }
    for child in &node.children {
        collect_notes(child, out);
    }
}

/// `$OLLAMA_MODELS`, or `~/.ollama/models`.
pub fn default_models_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OLLAMA_MODELS").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
    Some(Path::new(&home).join(".ollama").join("models"))
}

/// Text report of a `models` directory for the `--ollama` CLI: each model
/// with its blobs and inventory status. `Err` when `dir` is not a store.
pub fn ollama_report(dir: &Path) -> Result<(String, bool), String> {
    let store = OllamaStore::load(dir)
        .ok_or_else(|| format!("{}: no manifests directory", dir.display()))?;
    // Allocated bytes of each file in blobs/, as the scan counts them.
    let mut sizes: BTreeMap<String, u64> = BTreeMap::new();
    let io_error = |e: std::io::Error| format!("could not read blob inventory: {e}");
    for entry in fs::read_dir(dir.join("blobs")).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let meta = entry.metadata().map_err(io_error)?;
        if meta.is_file() {
            sizes.insert(
                entry.file_name().to_string_lossy().into_owned(),
                meta.blocks().saturating_mul(512),
            );
        }
    }
    let complete = store.unreadable.is_empty();
    let size_of = |blob: &str| sizes.get(blob).copied();
    let short = |blob: &str| blob.chars().take(19).collect::<String>();
    let mut out = format!("ollama_models_dir={}\n", dir.display());
    out.push_str(&format!("models={}\n", store.models.len()));
    out.push_str(&format!(
        "blobs_bytes={}\n\n",
        sizes.values().copied().fold(0u64, u64::saturating_add)
    ));

    let mut removable = Vec::new();
    for model in &store.models {
        let blobs: BTreeSet<&str> = model.layers.iter().map(|l| l.blob.as_str()).collect();
        let total: u64 = blobs.iter().filter_map(|b| size_of(b)).sum();
        // What removing this model alone gives back: blobs no other model uses.
        let own: u64 = blobs
            .iter()
            .filter(|b| store.uses(b).iter().all(|u| u.model == model.name))
            .filter_map(|b| size_of(b))
            .sum();
        if complete {
            removable.push((own, model.name.as_str()));
            out.push_str(&format!(
                "{}  {}  ({} exclusively referenced allocation)\n",
                model.name,
                format_bytes(total),
                format_bytes(own)
            ));
        } else {
            out.push_str(&format!(
                "{}  {}  (inventory incomplete)\n",
                model.name,
                format_bytes(total)
            ));
        }
        let mut seen = BTreeSet::new();
        for layer in &model.layers {
            if !seen.insert(layer.blob.as_str()) {
                continue;
            }
            let others: Vec<&str> = store
                .uses(&layer.blob)
                .iter()
                .map(|u| u.model.as_str())
                .filter(|m| *m != model.name)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let size = match size_of(&layer.blob) {
                Some(bytes) => format_bytes(bytes),
                None => "MISSING".to_string(),
            };
            let shared = if others.is_empty() {
                String::new()
            } else {
                format!("  shared with {}", others.join(", "))
            };
            out.push_str(&format!(
                "  {:>9}  {:<10} {}…{shared}\n",
                size,
                layer.kind,
                short(&layer.blob)
            ));
        }
    }

    let unused: Vec<(&String, &u64)> = sizes
        .iter()
        .filter(|(blob, _)| store.uses(blob).is_empty())
        .collect();
    let (partial, orphan): (Vec<_>, Vec<_>) = unused.into_iter().partition(|(b, _)| is_partial(b));
    for (title, list) in [
        (
            if complete {
                "Blobs no model uses"
            } else {
                "Blobs with unknown ownership"
            },
            &orphan,
        ),
        ("Unfinished downloads", &partial),
    ] {
        if list.is_empty() {
            continue;
        }
        let total = list
            .iter()
            .map(|(_, s)| **s)
            .fold(0u64, u64::saturating_add);
        out.push_str(&format!(
            "\n{title}: {} files, {}\n",
            list.len(),
            format_bytes(total)
        ));
        for (blob, size) in list.iter() {
            out.push_str(&format!("  {:>9}  {}\n", format_bytes(**size), blob));
        }
    }
    if complete && (!orphan.is_empty() || !partial.is_empty()) {
        out.push_str(
            "Ollama removes blobs no model uses when its server starts, unless OLLAMA_NOPRUNE is set.\n",
        );
    }
    let broken: Vec<&str> = store
        .models
        .iter()
        .filter(|m| m.layers.iter().any(|l| size_of(&l.blob).is_none()))
        .map(|m| m.name.as_str())
        .collect();
    if !broken.is_empty() {
        out.push_str(&format!(
            "\nModels with a missing blob (pull again or remove): {}\n",
            broken.join(", ")
        ));
    }
    for file in &store.unreadable {
        out.push_str(&format!(
            "\nIncomplete manifest inventory at: {}\n",
            file.display()
        ));
    }
    removable.sort_by(|a, b| b.cmp(a));
    if !removable.is_empty() {
        out.push_str("\nLargest first, by exclusively referenced allocation (actual space reclaimed may differ):\n");
        for (own, name) in removable {
            let command = remove_command(name)
                .unwrap_or_else(|| format!("{name:?} (unusual name: remove it by hand)"));
            out.push_str(&format!("  {:>9}  {command}\n", format_bytes(own)));
        }
    }
    Ok((out, complete))
}

/// Manifest files sit four levels down: host, namespace, model, tag.
fn manifest_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, unreadable: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        unreadable.push(dir.to_owned());
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            unreadable.push(dir.to_owned());
            continue;
        };
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            unreadable.push(path);
            continue;
        };
        if kind.is_dir() && depth < 8 {
            manifest_files(&path, depth + 1, out, unreadable);
        } else if kind.is_dir() || kind.is_symlink() {
            unreadable.push(path);
        } else if kind.is_file() && !entry.file_name().to_string_lossy().starts_with('.') {
            out.push(path);
        }
    }
}

/// `registry.ollama.ai/library/llama3/latest` reads `llama3:latest`, the way
/// `ollama list` shortens the default host and namespace.
fn model_name(manifests: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(manifests).unwrap_or(file);
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let tag = parts.pop().unwrap_or_default();
    if parts.first().map(String::as_str) == Some("registry.ollama.ai") {
        parts.remove(0);
        if parts.first().map(String::as_str) == Some("library") {
            parts.remove(0);
        }
    }
    if parts.is_empty() {
        tag
    } else {
        format!("{}:{tag}", parts.join("/"))
    }
}

fn read_manifest(path: &Path) -> Option<Vec<OllamaLayer>> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    read_manifest_layers(file)
}

fn read_manifest_layers(reader: impl Read) -> Option<Vec<OllamaLayer>> {
    let mut text = String::new();
    reader
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_MANIFEST_BYTES {
        return None;
    }
    manifest_layers(&text)
}

/// The config and layers of a manifest, or `None` when the text is not one.
fn manifest_layers(text: &str) -> Option<Vec<OllamaLayer>> {
    let top: serde_json::Value = serde_json::from_str(text).ok()?;
    fn within_limit(value: &serde_json::Value, depth: usize) -> bool {
        depth <= 32
            && match value {
                serde_json::Value::Array(a) => a.iter().all(|v| within_limit(v, depth + 1)),
                serde_json::Value::Object(o) => o.values().all(|v| within_limit(v, depth + 1)),
                _ => true,
            }
    }
    if !within_limit(&top, 0) {
        return None;
    }
    let mut out = Vec::new();
    let mut push = |value: &serde_json::Value, fallback: &str| -> Option<()> {
        let hex = value.get("digest")?.as_str()?.strip_prefix("sha256:")?;
        if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let kind = value
            .get("mediaType")
            .and_then(|v| v.as_str())
            .and_then(|v| v.strip_prefix("application/vnd.ollama.image."))
            .unwrap_or(fallback);
        out.push(OllamaLayer {
            blob: format!("sha256-{hex}"),
            kind: kind.to_owned(),
        });
        Some(())
    };
    if let Some(layers) = top.get("layers") {
        for layer in layers.as_array()? {
            push(layer, "layer")?;
        }
    }
    if let Some(config) = top.get("config") {
        push(config, "config")?;
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"schemaVersion":2,
      "mediaType":"application/vnd.docker.distribution.manifest.v2+json",
      "config":{"mediaType":"application/vnd.docker.container.image.v1+json","digest":"sha256:c0ffee","size":485},
      "layers":[
        {"mediaType":"application/vnd.ollama.image.model","digest":"sha256:ab12","size":4661211808},
        {"mediaType":"application/vnd.ollama.image.template","digest":"sha256:cd34","size":254}]}"#;

    #[test]
    fn manifest_read_stops_at_the_limit_plus_one_byte() {
        let manifest = br#"{"layers":[{"digest":"sha256:ab","mediaType":"application/vnd.ollama.image.\u6a21\u578b"}]}"#;
        let mut bytes = manifest.to_vec();
        bytes.resize(MAX_MANIFEST_BYTES as usize, b' ');
        assert_eq!(read_manifest_layers(&bytes[..]).unwrap()[0].kind, "模型");
        bytes.extend_from_slice(b"  ");
        let mut reader = std::io::Cursor::new(bytes);
        assert!(read_manifest_layers(&mut reader).is_none());
        assert_eq!(reader.position(), MAX_MANIFEST_BYTES + 1);
    }

    #[test]
    fn a_manifest_lists_its_layers_then_its_config() {
        let layers = manifest_layers(MANIFEST).unwrap();
        let pairs: Vec<(&str, &str)> = layers
            .iter()
            .map(|l| (l.blob.as_str(), l.kind.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("sha256-ab12", "model"),
                ("sha256-cd34", "template"),
                ("sha256-c0ffee", "config")
            ]
        );
        assert!(manifest_layers("{}").is_none());
        assert!(manifest_layers("not json").is_none());
        assert!(manifest_layers(r#"{"layers":[{"digest":"sha256:ab"}]} trailing"#).is_none());
    }

    #[test]
    fn model_names_read_as_ollama_list_prints_them() {
        let m = Path::new("/m/manifests");
        let name = |rel: &str| model_name(m, &m.join(rel));
        assert_eq!(
            name("registry.ollama.ai/library/llama3/latest"),
            "llama3:latest"
        );
        assert_eq!(name("registry.ollama.ai/me/tuned/v2"), "me/tuned:v2");
        assert_eq!(name("hf.co/org/repo/Q4_K_M"), "hf.co/org/repo:Q4_K_M");
    }

    #[test]
    fn blobs_are_labelled_by_model_shared_unused_or_partial() {
        let dir = std::env::temp_dir().join(format!("spacetree-ollama-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let lib = dir.join("manifests/registry.ollama.ai/library");
        fs::create_dir_all(lib.join("llama3")).unwrap();
        fs::create_dir_all(lib.join("tuned")).unwrap();
        fs::create_dir_all(dir.join("blobs")).unwrap();
        fs::write(lib.join("llama3/latest"), MANIFEST).unwrap();
        fs::write(
            lib.join("tuned/v1"),
            r#"{"layers":[{"mediaType":"application/vnd.ollama.image.model","digest":"sha256:ab12"}]}"#,
        )
        .unwrap();
        fs::write(lib.join("tuned/broken"), "{").unwrap();
        let store = OllamaStore::load(&dir).unwrap();
        assert_eq!(store.models.len(), 2);
        assert_eq!(store.unreadable.len(), 1);
        assert_eq!(
            blob_note(&store, "sha256-ab12").label,
            "llama3:latest +1 · model"
        );
        assert_eq!(
            blob_note(&store, "sha256-cd34").label,
            "llama3:latest · template"
        );
        assert_eq!(
            blob_note(&store, "sha256-ffff").label,
            "Ollama · ownership unknown"
        );
        assert_eq!(
            blob_note(&store, "sha256-ffff-partial-0").label,
            "Ollama · unfinished download"
        );
        for blob in ["sha256-ab12", "sha256-cd34", "sha256-ffff"] {
            fs::write(dir.join("blobs").join(blob), b"x").unwrap();
        }
        let (report, complete) = ollama_report(&dir).unwrap();
        assert!(!complete);
        fs::remove_dir_all(&dir).unwrap();
        assert!(report.contains("shared with tuned:v1"), "{report}");
        assert!(report.contains("MISSING"), "{report}");
        assert!(
            report.contains("Blobs with unknown ownership: 1 files"),
            "{report}"
        );
        assert!(!report.contains("ollama rm "));
        assert!(ollama_report(Path::new("/nonexistent-spacetree")).is_err());
        assert_eq!(
            remove_command("hf.co/org/repo:Q4_K_M").as_deref(),
            Some("ollama rm hf.co/org/repo:Q4_K_M")
        );
        for hostile in ["x; rm -rf ~", "$(id):1", "-rf", "a b", ""] {
            assert_eq!(remove_command(hostile), None, "{hostile}");
        }
    }
}
