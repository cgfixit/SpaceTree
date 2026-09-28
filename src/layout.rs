//! Integer squarified treemap. Area weights are allocated bytes. No egui.

use std::path::{Path, PathBuf};

use crate::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug)]
pub struct LayoutItem {
    pub path: PathBuf,
    pub weight: u64,
    pub color_ext: String,
    pub merged: bool,
    pub children: Vec<LayoutItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub path: PathBuf,
    pub rect: PxRect,
    pub color_ext: String,
    pub merged: bool,
}

#[derive(Clone, Debug)]
pub struct Tiling {
    bounds: PxRect,
    tiles: Vec<Tile>,
}

impl Tiling {
    pub fn bounds(&self) -> PxRect {
        self.bounds
    }

    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<&Path> {
        self.tiles
            .iter()
            .find(|t| contains(t.rect, x, y))
            .map(|t| t.path.as_path())
    }
}

fn contains(r: PxRect, x: i32, y: i32) -> bool {
    let x1 = r.x.saturating_add(r.w as i32);
    let y1 = r.y.saturating_add(r.h as i32);
    x >= r.x && y >= r.y && x < x1 && y < y1
}

/// Hamilton largest remainder. Ties go to the earlier index.
pub fn split_span(span: u32, weights: &[u64]) -> Vec<u32> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    if span == 0 {
        return vec![0; n];
    }
    let sum: u128 = weights.iter().map(|w| u128::from(*w)).sum();
    if sum == 0 {
        return vec![0; n];
    }
    let mut out = Vec::with_capacity(n);
    let mut rem = Vec::with_capacity(n);
    let mut used = 0u32;
    for w in weights {
        let prod = u128::from(span) * u128::from(*w);
        let q = (prod / sum) as u32;
        rem.push(prod % sum);
        out.push(q);
        used = used.saturating_add(q);
    }
    let mut left = span.saturating_sub(used);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| rem[b].cmp(&rem[a]).then(a.cmp(&b)));
    for i in order {
        if left == 0 {
            break;
        }
        out[i] = out[i].saturating_add(1);
        left -= 1;
    }
    out
}

pub fn layout_items(items: &[LayoutItem], bounds: PxRect) -> Tiling {
    let mut tiles = Vec::new();
    place_list(items, bounds, &mut tiles);
    Tiling { bounds, tiles }
}

pub fn layout_node(focus: &Node, bounds: PxRect) -> Tiling {
    let mut tiles = Vec::new();
    place_one_node(focus, bounds, &mut tiles);
    Tiling { bounds, tiles }
}

fn place_one_node(node: &Node, rect: PxRect, tiles: &mut Vec<Tile>) {
    if rect.w == 0 || rect.h == 0 || node.size == 0 {
        return;
    }
    if !node.is_dir || node.children.is_empty() || rect.w < 4 || rect.h < 4 {
        push_tile(tiles, &node.path, &node.color_ext, false, rect);
        return;
    }
    place_nodes(&node.children, rect, tiles);
}

pub fn treemap_focus<'a>(root: &'a Node, selected: Option<&Path>) -> &'a Node {
    let Some(path) = selected else {
        return root;
    };
    match find_with_parent(root, path) {
        Some((node, _)) if node.is_dir => node,
        Some((node, parent)) if !node.is_dir => parent.unwrap_or(root),
        _ => root,
    }
}

fn find_with_parent<'a>(root: &'a Node, path: &Path) -> Option<(&'a Node, Option<&'a Node>)> {
    fn rec<'a>(
        node: &'a Node,
        parent: Option<&'a Node>,
        path: &Path,
    ) -> Option<(&'a Node, Option<&'a Node>)> {
        if node.path == path {
            return Some((node, parent));
        }
        if node.is_dir && path.starts_with(&node.path) {
            for child in &node.children {
                if let Some(hit) = rec(child, Some(node), path) {
                    return Some(hit);
                }
            }
        }
        None
    }
    rec(root, None, path)
}

struct Part<'a> {
    path: &'a Path,
    weight: u64,
    color_ext: &'a str,
    merged: bool,
    kids: Kids<'a>,
}

enum Kids<'a> {
    Items(&'a [LayoutItem]),
    Nodes(&'a [Node]),
    Leaf,
}

fn tile_budget(rect: PxRect) -> usize {
    let px = u64::from(rect.w).saturating_mul(u64::from(rect.h));
    px.saturating_div(16).clamp(64, 1024) as usize
}

fn part_item(item: &LayoutItem) -> Part<'_> {
    Part {
        path: &item.path,
        weight: item.weight,
        color_ext: &item.color_ext,
        merged: item.merged,
        kids: if item.children.is_empty() {
            Kids::Leaf
        } else {
            Kids::Items(&item.children)
        },
    }
}

fn part_node(node: &Node) -> Part<'_> {
    Part {
        path: &node.path,
        weight: node.size,
        color_ext: &node.color_ext,
        merged: false,
        kids: if node.is_dir && !node.children.is_empty() {
            Kids::Nodes(&node.children)
        } else {
            Kids::Leaf
        },
    }
}

fn place_list(items: &[LayoutItem], bounds: PxRect, tiles: &mut Vec<Tile>) {
    let mut live: Vec<&LayoutItem> = items.iter().filter(|i| i.weight > 0).collect();
    live.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.path.cmp(&b.path)));
    let budget = tile_budget(bounds);
    if live.len() > budget {
        let dust_weight: u64 = live[budget - 1..].iter().map(|i| i.weight).sum();
        let donor = live[budget - 1];
        let dust = LayoutItem {
            path: donor.path.clone(),
            weight: dust_weight,
            color_ext: donor.color_ext.clone(),
            merged: true,
            children: Vec::new(),
        };
        let mut parts: Vec<Part<'_>> = live[..budget - 1].iter().copied().map(part_item).collect();
        parts.push(part_item(&dust));
        parts.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.path.cmp(b.path)));
        squarify(&parts, bounds, tiles);
        return;
    }
    let parts: Vec<Part<'_>> = live.iter().copied().map(part_item).collect();
    squarify(&parts, bounds, tiles);
}

fn place_nodes(nodes: &[Node], bounds: PxRect, tiles: &mut Vec<Tile>) {
    let mut live: Vec<&Node> = nodes.iter().filter(|n| n.size > 0).collect();
    live.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    let budget = tile_budget(bounds);
    if live.len() > budget {
        let dust_weight: u64 = live[budget - 1..].iter().map(|n| n.size).sum();
        let donor = live[budget - 1];
        let dust = LayoutItem {
            path: donor.path.clone(),
            weight: dust_weight,
            color_ext: donor.color_ext.clone(),
            merged: true,
            children: Vec::new(),
        };
        let mut parts: Vec<Part<'_>> = live[..budget - 1].iter().copied().map(part_node).collect();
        parts.push(part_item(&dust));
        parts.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.path.cmp(b.path)));
        squarify(&parts, bounds, tiles);
        return;
    }
    let parts: Vec<Part<'_>> = live.iter().copied().map(part_node).collect();
    squarify(&parts, bounds, tiles);
}

fn squarify(items: &[Part<'_>], rect: PxRect, tiles: &mut Vec<Tile>) {
    if items.is_empty() || rect.w == 0 || rect.h == 0 {
        return;
    }
    if items.len() == 1 {
        place_one(&items[0], rect, tiles);
        return;
    }
    let mut start = 0;
    let mut area = rect;
    while start < items.len() {
        if area.w == 0 || area.h == 0 {
            return;
        }
        let rest = &items[start..];
        if rest.len() == 1 {
            place_one(&rest[0], area, tiles);
            return;
        }
        let n = choose_row(rest, area);
        let row = &rest[..n];
        let last = n == rest.len();
        let (row_rect, leftover) = strip(area, weight_of(row), weight_of(rest), last);
        commit_row(row, row_rect, area.w >= area.h, tiles);
        start += n;
        area = leftover;
    }
}

fn place_one(part: &Part<'_>, rect: PxRect, tiles: &mut Vec<Tile>) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let tiny = rect.w < 4 || rect.h < 4;
    if tiny || matches!(part.kids, Kids::Leaf) {
        push_tile(tiles, part.path, part.color_ext, part.merged, rect);
        return;
    }
    match part.kids {
        Kids::Items(children) => place_list(children, rect, tiles),
        Kids::Nodes(children) => place_nodes(children, rect, tiles),
        Kids::Leaf => {}
    }
}

fn push_tile(tiles: &mut Vec<Tile>, path: &Path, color_ext: &str, merged: bool, rect: PxRect) {
    tiles.push(Tile {
        path: path.to_path_buf(),
        color_ext: color_ext.to_string(),
        merged,
        rect,
    });
}

fn choose_row(rest: &[Part<'_>], rect: PxRect) -> usize {
    let mut n = 1;
    let mut prev = worst_aspect(&rest[..1], rect, rest);
    while n < rest.len() {
        let next = worst_aspect(&rest[..=n], rect, rest);
        if worse(next, prev) {
            break;
        }
        prev = next;
        n += 1;
    }
    n
}

fn worst_aspect(row: &[Part<'_>], rect: PxRect, rest: &[Part<'_>]) -> (u128, u128) {
    let along_y = rect.w >= rect.h;
    let last = row.len() == rest.len();
    let (row_rect, _) = strip(rect, weight_of(row), weight_of(rest), last);
    let rects = split_row(row, row_rect, along_y);
    let mut worst = (1u128, 1u128);
    let mut any = false;
    for r in rects {
        let a = aspect(r.w, r.h);
        if !any || worse(a, worst) {
            worst = a;
            any = true;
        }
    }
    if any {
        worst
    } else {
        (u128::from(u32::MAX), 1)
    }
}

fn aspect(w: u32, h: u32) -> (u128, u128) {
    if w == 0 || h == 0 {
        (u128::from(u32::MAX), 1)
    } else if w >= h {
        (u128::from(w), u128::from(h))
    } else {
        (u128::from(h), u128::from(w))
    }
}

fn worse(a: (u128, u128), b: (u128, u128)) -> bool {
    a.0.saturating_mul(b.1) > b.0.saturating_mul(a.1)
}

fn weight_of(items: &[Part<'_>]) -> u128 {
    items.iter().map(|i| u128::from(i.weight)).sum()
}

fn strip(rect: PxRect, row_w: u128, rem_w: u128, last: bool) -> (PxRect, PxRect) {
    if rect.w >= rect.h {
        let span = rect.w;
        let thick = thickness(span, row_w, rem_w, last);
        (
            PxRect {
                x: rect.x,
                y: rect.y,
                w: thick,
                h: rect.h,
            },
            PxRect {
                x: rect.x.saturating_add(thick as i32),
                y: rect.y,
                w: span - thick,
                h: rect.h,
            },
        )
    } else {
        let span = rect.h;
        let thick = thickness(span, row_w, rem_w, last);
        (
            PxRect {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: thick,
            },
            PxRect {
                x: rect.x,
                y: rect.y.saturating_add(thick as i32),
                w: rect.w,
                h: span - thick,
            },
        )
    }
}

fn thickness(span: u32, row_w: u128, rem_w: u128, last: bool) -> u32 {
    if last || rem_w == 0 {
        return span;
    }
    let t = ((row_w.saturating_mul(u128::from(span))) / rem_w) as u32;
    t.min(span)
}

fn split_row(row: &[Part<'_>], row_rect: PxRect, along_y: bool) -> Vec<PxRect> {
    let weights: Vec<u64> = row.iter().map(|i| i.weight).collect();
    if along_y {
        let spans = split_span(row_rect.h, &weights);
        let mut y = row_rect.y;
        spans
            .into_iter()
            .map(|h| {
                let r = PxRect {
                    x: row_rect.x,
                    y,
                    w: row_rect.w,
                    h,
                };
                y = y.saturating_add(h as i32);
                r
            })
            .collect()
    } else {
        let spans = split_span(row_rect.w, &weights);
        let mut x = row_rect.x;
        spans
            .into_iter()
            .map(|w| {
                let r = PxRect {
                    x,
                    y: row_rect.y,
                    w,
                    h: row_rect.h,
                };
                x = x.saturating_add(w as i32);
                r
            })
            .collect()
    }
}

fn commit_row(row: &[Part<'_>], row_rect: PxRect, along_y: bool, tiles: &mut Vec<Tile>) {
    for (item, rect) in row.iter().zip(split_row(row, row_rect, along_y)) {
        place_one(item, rect, tiles);
    }
}
