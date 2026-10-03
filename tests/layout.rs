use std::path::{Path, PathBuf};
use std::time::SystemTime;

use spacetree::{layout_items, split_span, treemap_focus, LayoutItem, Node, PxRect};

fn item(name: &str, weight: u64) -> LayoutItem {
    LayoutItem {
        path: PathBuf::from(name),
        weight,
        color_ext: String::new(),
        merged: false,
        children: Vec::new(),
    }
}

fn dir(name: &str, weight: u64, children: Vec<LayoutItem>) -> LayoutItem {
    LayoutItem {
        path: PathBuf::from(name),
        weight,
        color_ext: String::new(),
        merged: false,
        children,
    }
}

fn rect(x: i32, y: i32, w: u32, h: u32) -> PxRect {
    PxRect { x, y, w, h }
}

fn tile<'a>(tiling: &'a spacetree::Tiling, name: &str) -> &'a PxRect {
    &tiling
        .tiles()
        .iter()
        .find(|t| t.path.as_path() == Path::new(name))
        .unwrap_or_else(|| panic!("missing {name}"))
        .rect
}

fn assert_partition(tiling: &spacetree::Tiling, w: i32, h: i32) {
    for y in 0..h {
        for x in 0..w {
            let hits = tiling
                .tiles()
                .iter()
                .filter(|t| {
                    x >= t.rect.x
                        && y >= t.rect.y
                        && x < t.rect.x + t.rect.w as i32
                        && y < t.rect.y + t.rect.h as i32
                })
                .count();
            assert_eq!(hits, 1, "point ({x},{y}) hit {hits} tiles");
        }
    }
}

#[test]
fn hamilton_remainder_prefers_the_earlier_index() {
    assert_eq!(split_span(10, &[1, 1, 1]), vec![4, 3, 3]);
    assert_eq!(split_span(5, &[1, 1, 1]), vec![2, 2, 1]);
    assert_eq!(split_span(10, &[5, 1, 1]), vec![7, 2, 1]);
}

#[test]
fn two_equal_files_split_a_wide_rect_and_ignore_input_order() {
    let bounds = rect(0, 0, 10, 4);
    for input in [
        vec![item("b", 1), item("a", 1)],
        vec![item("a", 1), item("b", 1)],
    ] {
        let tiling = layout_items(&input, bounds);
        assert_eq!(*tile(&tiling, "a"), rect(0, 0, 5, 4));
        assert_eq!(*tile(&tiling, "b"), rect(5, 0, 5, 4));
        assert_eq!(tiling.hit(5, 0).unwrap(), std::path::Path::new("b"));
        assert!(tiling.hit(10, 0).is_none());
        assert_partition(&tiling, 10, 4);
    }
}

#[test]
fn three_equal_files_match_the_worked_wide_layout() {
    let tiling = layout_items(
        &[item("c", 1), item("a", 1), item("b", 1)],
        rect(0, 0, 10, 6),
    );
    assert_eq!(*tile(&tiling, "a"), rect(0, 0, 6, 3));
    assert_eq!(*tile(&tiling, "b"), rect(0, 3, 6, 3));
    assert_eq!(*tile(&tiling, "c"), rect(6, 0, 4, 6));
    assert_partition(&tiling, 10, 6);
}

#[test]
fn zero_weight_sibling_gets_no_tile() {
    let tiling = layout_items(&[item("gone", 0), item("keep", 5)], rect(0, 0, 8, 3));
    assert_eq!(tiling.tiles().len(), 1);
    assert_eq!(*tile(&tiling, "keep"), rect(0, 0, 8, 3));
    assert_partition(&tiling, 8, 3);
}

#[test]
fn directories_are_frames_not_tiles() {
    let tiling = layout_items(
        &[
            dir("left", 1, vec![item("a", 1)]),
            dir("right", 1, vec![item("b", 1)]),
        ],
        rect(0, 0, 10, 4),
    );
    assert_eq!(tiling.tiles().len(), 2);
    assert_eq!(*tile(&tiling, "a"), rect(0, 0, 5, 4));
    assert_eq!(*tile(&tiling, "b"), rect(5, 0, 5, 4));
    assert!(tiling
        .tiles()
        .iter()
        .all(|t| t.path.as_path() != Path::new("left")));
}

fn tree_node(path: &str, is_dir: bool, children: Vec<Node>) -> Node {
    Node {
        name: Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string()),
        path: PathBuf::from(path),
        is_dir,
        size: 1,
        logical: 1,
        files: if is_dir { 0 } else { 1 },
        color_ext: "txt".to_string(),
        modified: None::<SystemTime>,
        created: None,
        percent_of_disk: 0.0,
        children,
    }
}

#[test]
fn file_selection_focuses_the_parent_directory() {
    let file = tree_node("/scan/a.txt", false, Vec::new());
    let root = tree_node("/scan", true, vec![file]);
    assert_eq!(
        treemap_focus(&root, Some(Path::new("/scan/a.txt"))).path,
        Path::new("/scan")
    );
    assert_eq!(
        treemap_focus(&root, Some(Path::new("/scan"))).path,
        Path::new("/scan")
    );
    assert_eq!(treemap_focus(&root, None).path, Path::new("/scan"));
}

#[test]
fn directories_record_nested_frames_around_their_tiles() {
    let tiling = layout_items(
        &[
            dir(
                "left",
                2,
                vec![dir("left/in", 2, vec![item("left/in/a", 2)])],
            ),
            dir("right", 2, vec![item("right/b", 1), item("right/c", 1)]),
        ],
        rect(0, 0, 20, 8),
    );
    let frames: Vec<(&Path, PxRect, u64, u32)> = tiling
        .frames()
        .iter()
        .map(|f| (f.path.as_path(), f.rect, f.weight, f.depth))
        .collect();
    assert_eq!(
        frames,
        vec![
            (Path::new("left"), rect(0, 0, 10, 8), 2, 1),
            (Path::new("left/in"), rect(0, 0, 10, 8), 2, 2),
            (Path::new("right"), rect(10, 0, 10, 8), 2, 1),
        ]
    );
    // Every tile sits inside the frame of each ancestor directory.
    for t in tiling.tiles() {
        for f in tiling.frames() {
            if t.path.starts_with(&f.path) {
                assert!(t.rect.x >= f.rect.x && t.rect.y >= f.rect.y);
                assert!(t.rect.x + t.rect.w as i32 <= f.rect.x + f.rect.w as i32);
                assert!(t.rect.y + t.rect.h as i32 <= f.rect.y + f.rect.h as i32);
            }
        }
    }
    let under: Vec<&Path> = tiling.frames_at(1, 1).map(|f| f.path.as_path()).collect();
    assert_eq!(under, vec![Path::new("left"), Path::new("left/in")]);
    assert_partition(&tiling, 20, 8);
}

#[test]
fn tiles_carry_their_weight_and_a_merged_tile_carries_the_sum() {
    let tiling = layout_items(&[item("big", 7), item("small", 3)], rect(0, 0, 10, 4));
    let weight = |name: &str| {
        tiling
            .tiles()
            .iter()
            .find(|t| t.path.as_path() == Path::new(name))
            .map(|t| t.weight)
    };
    assert_eq!(weight("big"), Some(7));
    assert_eq!(weight("small"), Some(3));
    assert_eq!(
        tiling.hit_tile(0, 0).map(|t| t.path.as_path()),
        Some(Path::new("big"))
    );

    // 32x32 px allows 64 tiles; 100 files leave 37 in one merged tile.
    let many: Vec<LayoutItem> = (0..100)
        .map(|i| item(&format!("f{i:03}"), 1000 - i as u64))
        .collect();
    let total: u64 = many.iter().map(|i| i.weight).sum();
    let tiling = layout_items(&many, rect(0, 0, 32, 32));
    let merged: Vec<_> = tiling.tiles().iter().filter(|t| t.merged).collect();
    assert_eq!(merged.len(), 1);
    let placed: u64 = tiling.tiles().iter().map(|t| t.weight).sum();
    assert_eq!(placed, total, "tile weights must account for every byte");
    assert_eq!(
        merged[0].weight,
        (63..100).map(|i| 1000 - i as u64).sum::<u64>()
    );
}
