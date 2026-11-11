//! Disk-usage tree: parallel walk, allocated and logical sizes, and a treemap layout.

pub mod app;
mod ext;
mod extents;
mod finder;
mod format;
mod layout;
mod ollama;
mod scan;
mod sort;

pub use ext::{
    ext_color, ext_description, ext_key, ext_label, format_scan_share, legend_of, share_px,
    LegendRow, Rgb, HIDDEN_KEY,
};
pub use format::{format_bytes, format_scan, format_scan_to_depth, format_volume};
pub use layout::{
    layout_items, layout_node, split_span, treemap_focus, Frame, LayoutItem, PxRect, Tile, Tiling,
};
pub use ollama::{
    blob_notes, default_models_dir, format_ollama, BlobNote, BlobUse, OllamaLayer, OllamaModel,
    OllamaStore,
};
pub use scan::{scan, Node, ScanResult, BLOCKED_SAMPLE, DATA_ONLY_NAME, HIDDEN_NAME};
pub use sort::{sort_children, sort_tree, SortColumn};
