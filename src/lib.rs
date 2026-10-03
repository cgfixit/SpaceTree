//! Disk-usage tree: parallel walk, allocated and logical sizes, and a treemap layout.

pub mod app;
mod ext;
mod extents;
mod finder;
mod format;
mod layout;
mod scan;
mod sort;

pub use ext::{
    ext_color, ext_description, ext_key, ext_label, format_scan_share, legend_of, share_px,
    LegendRow, Rgb,
};
pub use format::{format_bytes, format_scan};
pub use layout::{
    layout_items, layout_node, split_span, treemap_focus, Frame, LayoutItem, PxRect, Tile, Tiling,
};
pub use scan::{scan, Node, ScanResult};
pub use sort::{sort_children, sort_tree, SortColumn};
