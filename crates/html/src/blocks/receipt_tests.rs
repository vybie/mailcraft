// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn table(blocks: &[Block]) -> &Table {
    blocks.iter().find_map(|block| match &block.kind {
        BlockKind::Table(table) => Some(table),
        _ => None,
    }).expect("table")
}

#[test]
fn receipt_wrappers_keep_cell_widths() {
    let blocks = parse("<table role='presentation' width='100%'><tr><td style='max-width:612px'>\
        <table role='presentation' width='100%'><tr><td width='578'>\
        <table role='presentation' width='568'><tr><td>Receipt</td></tr></table>\
        </td></tr></table></td></tr></table>");
    let outer = table(&blocks);
    assert_eq!(outer.rows[0].cells[0].width, Some(Length::Px(612)));
    let inner = table(&outer.rows[0].cells[0].blocks);
    assert_eq!(inner.rows[0].cells[0].width, Some(Length::Px(578)));
    let receipt = table(&inner.rows[0].cells[0].blocks);
    assert_eq!(receipt.width, Some(Length::Px(568)));
}

#[test]
fn wrapper_without_cell_width_is_still_flattened() {
    let blocks = parse("<table role='presentation' width='100%'><tr><td>Receipt</td></tr></table>");
    assert!(blocks.iter().all(|block| !matches!(block.kind, BlockKind::Table(_))));
    assert!(crate::has_text(&blocks));
}
