use crate::config::{Anchor, General, Output};

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

/// Where a stack of notifications is: `[general]`'s anchor and output, or a
/// rule's. Each placement is a stack of its own; stacks with the same anchor
/// on the same output overlap.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Placement {
    pub anchor: Anchor,
    pub output: Output,
}

impl Placement {
    pub fn of(general: &General) -> Self {
        Self {
            anchor: general.anchor,
            output: general.output.clone(),
        }
    }
}

/// Margins for several stacks at once: `items` are (placement, height) in
/// display order; the items of each placement are stacked as by
/// [`stack_margins`], independently of the others.
pub fn stacks_margins(general: &General, items: &[(&Placement, u32)]) -> Vec<Margin> {
    let mut margins = vec![(0, 0, 0, 0); items.len()];
    let mut done = vec![false; items.len()];
    for i in 0..items.len() {
        if done[i] {
            continue;
        }
        let placement = items[i].0;
        let members: Vec<usize> = (i..items.len())
            .filter(|&j| items[j].0 == placement)
            .collect();
        let heights: Vec<u32> = members.iter().map(|&j| items[j].1).collect();
        let place = General {
            anchor: placement.anchor,
            ..general.clone()
        };
        for (&j, margin) in members.iter().zip(stack_margins(&place, &heights)) {
            margins[j] = margin;
            done[j] = true;
        }
    }
    margins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_placement_is_its_own_stack() {
        let g = general(Anchor::TopRight);
        let main = Placement::of(&g);
        let bottom = Placement {
            anchor: Anchor::Bottom,
            ..main.clone()
        };
        let other_output = Placement {
            output: Output::Name("DP-2".into()),
            ..main.clone()
        };
        assert_eq!(
            stacks_margins(
                &g,
                &[
                    (&main, 100),
                    (&bottom, 40),
                    (&main, 50),
                    (&other_output, 60),
                    (&bottom, 10)
                ]
            ),
            vec![
                (30, 20, 0, 0),
                (0, 0, 30, 0),
                (138, 20, 0, 0),
                (30, 20, 0, 0),
                (0, 0, 78, 0)
            ]
        );
    }

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
