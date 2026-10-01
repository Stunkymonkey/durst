use crate::config::{Anchor, General};

/// Layer-shell margin in the order the protocol uses: top, right, bottom, left.
pub type Margin = (i32, i32, i32, i32);

/// Margins for a stack of surfaces. `heights[0]` is the surface closest to
/// the anchored screen edge, every following one is placed `gap` further away.
pub fn stack_margins(general: &General, heights: &[u32]) -> Vec<Margin> {
    let [x, y] = general.offset;
    let (left, right) = match general.anchor {
        Anchor::TopLeft | Anchor::BottomLeft => (x, 0),
        Anchor::TopRight | Anchor::BottomRight => (0, x),
        Anchor::Top | Anchor::Bottom => (0, 0),
    };
    let mut distance = y;
    heights
        .iter()
        .map(|&h| {
            let margin = if general.anchor.is_top() {
                (distance, right, 0, left)
            } else {
                (0, right, distance, left)
            };
            distance += (h + general.gap) as i32;
            margin
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn general(anchor: Anchor) -> General {
        General {
            anchor,
            offset: [20, 30],
            gap: 8,
            ..General::default()
        }
    }

    #[test]
    fn top_right_stacks_down() {
        assert_eq!(
            stack_margins(&general(Anchor::TopRight), &[100, 50, 70]),
            vec![(30, 20, 0, 0), (138, 20, 0, 0), (196, 20, 0, 0)]
        );
    }

    #[test]
    fn bottom_left_stacks_up() {
        assert_eq!(
            stack_margins(&general(Anchor::BottomLeft), &[100, 50]),
            vec![(0, 0, 30, 20), (0, 0, 138, 20)]
        );
    }

    #[test]
    fn centered_has_no_horizontal_margin() {
        assert_eq!(
            stack_margins(&general(Anchor::Top), &[10]),
            vec![(30, 0, 0, 0)]
        );
    }
}
