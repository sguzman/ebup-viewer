#![allow(dead_code)]

use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct PdfViewportPlanInput {
    pub total_pages: usize,
    pub visible_page_indexes: Vec<usize>,
    pub overscan: usize,
    pub active_tts_page_index: Option<usize>,
    pub jump_target_page_index: Option<usize>,
}

#[derive(Default, Clone, Debug)]
pub struct PdfViewportRenderPlan {
    pub canvas_page_indexes: Vec<usize>,
    pub text_layer_page_indexes: Vec<usize>,
    pub priority_page_indexes: Vec<usize>,
    pub medium_priority_page_indexes: Vec<usize>,
    pub low_priority_page_indexes: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct PdfPageRegistryEntry {
    pub page_index: usize,
    pub last_touched_at: u64,
    pub rendered_zoom: Option<f32>,
    pub text_layer_zoom: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct PdfViewportBudgetInput {
    pub entries: Vec<PdfPageRegistryEntry>,
    pub keep_canvas_page_indexes: Vec<usize>,
    pub keep_text_layer_page_indexes: Vec<usize>,
    pub max_canvas_pages: usize,
    pub max_text_layer_pages: usize,
}

#[derive(Clone, Debug)]
pub struct PdfViewportBudgetDecision {
    pub evict_canvas_page_indexes: Vec<usize>,
    pub evict_text_layer_page_indexes: Vec<usize>,
}

pub fn build_pdf_viewport_render_plan(input: &PdfViewportPlanInput) -> PdfViewportRenderPlan {
    let total_pages = input.total_pages;
    if total_pages == 0 {
        return PdfViewportRenderPlan::default();
    }
    let mut visible = input
        .visible_page_indexes
        .iter()
        .copied()
        .filter(|&page_index| page_index < total_pages)
        .collect::<Vec<_>>();
    visible = sort_unique(visible);

    let mut priority = Vec::new();
    if let Some(target) = input.jump_target_page_index {
        priority.push(clamp_page(target, total_pages));
    }
    if let Some(active_page) = input.active_tts_page_index {
        priority.push(clamp_page(active_page, total_pages));
    }
    if !visible.is_empty() {
        let center_idx = visible[visible.len() / 2];
        priority.push(clamp_page(center_idx, total_pages));
    }
    let priority_page_indexes = sort_unique(priority);

    let canvas_page_indexes = sort_unique(
        visible
            .iter()
            .copied()
            .flat_map(|page_index| page_window(page_index, total_pages, input.overscan))
            .chain(priority_page_indexes.iter().copied())
            .collect(),
    );

    let medium_radius = input.overscan.max(1);
    let medium_priority_page_indexes = {
        let filtered = sort_unique(
            visible
                .iter()
                .copied()
                .flat_map(|page_index| page_window(page_index, total_pages, medium_radius))
                .chain(priority_page_indexes.iter().copied())
                .collect(),
        );
        filtered
            .into_iter()
            .filter(|page_index| !priority_page_indexes.contains(page_index))
            .collect::<Vec<_>>()
    };

    let mut text_layer_page_indexes = visible.clone();
    if let Some(active_page) = input.active_tts_page_index {
        text_layer_page_indexes.push(clamp_page(active_page, total_pages));
    }
    if let Some(jump) = input.jump_target_page_index {
        text_layer_page_indexes.push(clamp_page(jump, total_pages));
    }
    let text_layer_page_indexes = sort_unique(text_layer_page_indexes);

    let low_priority_page_indexes = sort_unique(
        visible
            .iter()
            .copied()
            .flat_map(|page_index| page_window(page_index, total_pages, input.overscan + 2))
            .collect(),
    )
    .into_iter()
    .filter(|page_index| {
        !priority_page_indexes.contains(page_index)
            && !medium_priority_page_indexes.contains(page_index)
    })
    .collect();

    PdfViewportRenderPlan {
        canvas_page_indexes,
        text_layer_page_indexes,
        priority_page_indexes,
        medium_priority_page_indexes,
        low_priority_page_indexes,
    }
}

pub fn choose_pdf_viewport_evictions(input: &PdfViewportBudgetInput) -> PdfViewportBudgetDecision {
    let keep_canvas: HashSet<usize> = input.keep_canvas_page_indexes.iter().copied().collect();
    let keep_text: HashSet<usize> = input.keep_text_layer_page_indexes.iter().copied().collect();

    let mut canvas_candidates: Vec<&PdfPageRegistryEntry> = input
        .entries
        .iter()
        .filter(|entry| entry.rendered_zoom.is_some())
        .collect();
    canvas_candidates.sort_by(|left, right| {
        eviction_priority(left, &keep_canvas, EvictionMode::Canvas).cmp(&eviction_priority(
            right,
            &keep_canvas,
            EvictionMode::Canvas,
        ))
    });

    let mut text_candidates: Vec<&PdfPageRegistryEntry> = input
        .entries
        .iter()
        .filter(|entry| entry.text_layer_zoom.is_some())
        .collect();
    text_candidates.sort_by(|left, right| {
        eviction_priority(left, &keep_text, EvictionMode::Text).cmp(&eviction_priority(
            right,
            &keep_text,
            EvictionMode::Text,
        ))
    });

    let canvas_overflow = canvas_candidates
        .len()
        .saturating_sub(input.max_canvas_pages);
    let text_overflow = text_candidates
        .len()
        .saturating_sub(input.max_text_layer_pages);

    let evict_canvas_page_indexes = sort_unique(
        canvas_candidates
            .iter()
            .filter(|entry| !keep_canvas.contains(&entry.page_index))
            .take(canvas_overflow)
            .map(|entry| entry.page_index)
            .collect(),
    );

    let evict_text_layer_page_indexes = sort_unique(
        text_candidates
            .iter()
            .filter(|entry| !keep_text.contains(&entry.page_index))
            .take(text_overflow)
            .map(|entry| entry.page_index)
            .collect(),
    );

    PdfViewportBudgetDecision {
        evict_canvas_page_indexes,
        evict_text_layer_page_indexes,
    }
}

#[derive(Debug, Clone, Copy)]
enum EvictionMode {
    Canvas,
    Text,
}

fn clamp_page(page_index: usize, total_pages: usize) -> usize {
    if total_pages == 0 {
        return 0;
    }
    page_index.min(total_pages.saturating_sub(1))
}

fn sort_unique(mut values: Vec<usize>) -> Vec<usize> {
    values.sort_unstable();
    values.dedup();
    values
}

fn page_window(page_index: usize, total_pages: usize, radius: usize) -> Vec<usize> {
    if total_pages == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let base = page_index as isize;
    for delta in -(radius as isize)..=(radius as isize) {
        let candidate = base + delta;
        if candidate < 0 {
            continue;
        }
        let candidate = candidate as usize;
        out.push(clamp_page(candidate, total_pages));
    }
    sort_unique(out)
}

fn eviction_priority(
    entry: &PdfPageRegistryEntry,
    keep: &HashSet<usize>,
    mode: EvictionMode,
) -> i64 {
    if keep.contains(&entry.page_index) {
        return i64::MIN;
    }
    let has_artifact = match mode {
        EvictionMode::Canvas => entry.rendered_zoom.is_some(),
        EvictionMode::Text => entry.text_layer_zoom.is_some(),
    };
    if !has_artifact {
        return i64::MIN;
    }
    entry.last_touched_at as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_plan_prioritizes_jump_and_tts_pages() {
        let input = PdfViewportPlanInput {
            total_pages: 12,
            visible_page_indexes: vec![5],
            overscan: 1,
            active_tts_page_index: Some(2),
            jump_target_page_index: Some(9),
        };
        let plan = build_pdf_viewport_render_plan(&input);
        assert!(plan.priority_page_indexes.contains(&5));
        assert!(plan.priority_page_indexes.contains(&2));
        assert!(plan.priority_page_indexes.contains(&9));
        assert!(plan.canvas_page_indexes.contains(&4));
        assert!(plan.canvas_page_indexes.contains(&6));
    }

    #[test]
    fn viewport_eviction_preserves_keep_pages() {
        let entries = (0..6)
            .map(|page_index| PdfPageRegistryEntry {
                page_index,
                last_touched_at: page_index as u64,
                rendered_zoom: Some(1.0),
                text_layer_zoom: Some(1.0),
            })
            .collect::<Vec<_>>();
        let decision = choose_pdf_viewport_evictions(&PdfViewportBudgetInput {
            entries,
            keep_canvas_page_indexes: vec![0, 1],
            keep_text_layer_page_indexes: vec![0],
            max_canvas_pages: 2,
            max_text_layer_pages: 1,
        });
        assert!(!decision.evict_canvas_page_indexes.contains(&0));
        assert!(!decision.evict_canvas_page_indexes.contains(&1));
        assert!(!decision.evict_text_layer_page_indexes.contains(&0));
    }
}
