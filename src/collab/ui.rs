use crate::{buffer::rope::EditorBuffer, collab::presence::PeerPresence};

const PEER_COLORS: [&str; 6] = [
    "\x1b[36m", "\x1b[33m", "\x1b[35m", "\x1b[32m", "\x1b[34m", "\x1b[91m",
];

pub fn peer_color_index(peer_id: &uuid::Uuid) -> usize {
    (peer_id.as_u128() % PEER_COLORS.len() as u128) as usize
}

pub fn peer_color(peer_id: &uuid::Uuid) -> &'static str {
    PEER_COLORS[peer_color_index(peer_id)]
}

pub fn caret_marker(peer_id: &uuid::Uuid) -> String {
    format!("{}▏\x1b[0m", peer_color(peer_id))
}

pub fn overlay_remote_selections(
    buffer: &EditorBuffer,
    top_line: usize,
    left_col: usize,
    lines: Vec<String>,
    peers: &[PeerPresence],
) -> Vec<String> {
    if peers.is_empty() {
        return lines;
    }

    lines
        .into_iter()
        .enumerate()
        .map(|(visible_idx, line)| {
            let line_no = top_line + visible_idx;
            let mut ranges: Vec<(usize, usize, String)> = Vec::new();
            for peer in peers {
                for &(anchor, head) in &peer.selections {
                    let Some((start_col, end_col)) =
                        selection_cols_on_line(buffer, anchor, head, line_no, left_col)
                    else {
                        continue;
                    };
                    if start_col < end_col {
                        ranges.push((
                            start_col,
                            end_col,
                            format!("{}\x1b[7m", peer_color(&peer.peer_id)),
                        ));
                    }
                }
            }
            if ranges.is_empty() {
                return line;
            }
            ranges.sort_by(|left, right| right.0.cmp(&left.0));
            overlay_ranges(&line, &ranges)
        })
        .collect()
}

pub fn overlay_remote_carets(
    buffer: &EditorBuffer,
    top_line: usize,
    left_col: usize,
    lines: Vec<String>,
    peers: &[PeerPresence],
) -> Vec<String> {
    if peers.is_empty() {
        return lines;
    }

    lines
        .into_iter()
        .enumerate()
        .map(|(visible_idx, line)| {
            let line_no = top_line + visible_idx;
            let mut markers: Vec<(usize, String)> = Vec::new();
            for peer in peers {
                let Some(head) = peer.primary_head() else {
                    continue;
                };
                let (row, col) = buffer.char_to_line_col(head);
                if row != line_no || col < left_col {
                    continue;
                }
                let display_col = col.saturating_sub(left_col);
                markers.push((display_col, caret_marker(&peer.peer_id)));
            }
            if markers.is_empty() {
                return line;
            }
            markers.sort_by(|left, right| right.0.cmp(&left.0));
            insert_markers(&line, &markers)
        })
        .collect()
}

pub fn peer_summary(buffer: &EditorBuffer, peers: &[PeerPresence]) -> String {
    peers
        .iter()
        .map(|peer| {
            let (row, col) = peer
                .primary_head()
                .map(|head| buffer.char_to_line_col(head))
                .unwrap_or((0, 0));
            format!(
                "{}{}@{}:{}\x1b[0m",
                peer_color(&peer.peer_id),
                peer.name,
                row + 1,
                col + 1
            )
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn selection_cols_on_line(
    buffer: &EditorBuffer,
    anchor: usize,
    head: usize,
    line_no: usize,
    left_col: usize,
) -> Option<(usize, usize)> {
    let start = anchor.min(head);
    let end = anchor.max(head);
    let (start_row, start_col) = buffer.char_to_line_col(start);
    let (end_row, end_col) = buffer.char_to_line_col(end);
    if line_no < start_row || line_no > end_row {
        return None;
    }
    let line_len = buffer.line_string(line_no).chars().count();
    let col_start = if line_no == start_row { start_col } else { 0 };
    let col_end = if line_no == end_row {
        end_col.max(start_col)
    } else {
        line_len
    };
    if col_end <= left_col {
        return None;
    }
    let display_start = col_start.saturating_sub(left_col);
    let display_end = col_end.saturating_sub(left_col);
    Some((display_start, display_end))
}

fn overlay_ranges(line: &str, ranges: &[(usize, usize, String)]) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut cursor = 0usize;
    for &(start, end, ref open) in ranges {
        let start = start.min(chars.len());
        let end = end.min(chars.len());
        if end <= start || start < cursor {
            continue;
        }
        out.extend(chars[cursor..start].iter());
        out.push_str(open);
        out.extend(chars[start..end].iter());
        out.push_str("\x1b[0m");
        cursor = end;
    }
    out.extend(chars[cursor..].iter());
    out
}

fn insert_markers(line: &str, markers: &[(usize, String)]) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut cursor = 0usize;
    for &(col, ref marker) in markers {
        let col = col.min(chars.len());
        if col < cursor {
            continue;
        }
        out.extend(chars[cursor..col].iter());
        out.push_str(marker);
        cursor = col;
    }
    out.extend(chars[cursor..].iter());
    out
}
