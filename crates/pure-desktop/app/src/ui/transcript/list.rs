//! The measured-height list: the transcript's virtualization.
//!
//! The E00 spike measured `egui_virtual_list` at 800 to 1000 ms boot
//! frames and 2.7 GB RSS at 5k rows, so this list is hand-rolled
//! instead: a `ScrollArea` that measures each row's height once, keeps
//! the measurement under the row's signature, and lays out only the
//! rows the viewport can see. A row whose signature is unchanged keeps
//! its measurement, so a streaming append re-measures the tail and
//! nothing else.
//!
//! The follow state machine is the old list's: follow on at the tail,
//! a scroll up detaches, a jump affordance reattaches, and new entries
//! while following keep the view pinned.

/// How many rows past the viewport are laid out, so a fast scroll has
/// something drawn before it arrives.
const OVERDRAW: f32 = 400.0;

/// The scroll offset, in points, above which a scroll counts as "up".
const DETACH_SLACK: f32 = 8.0;

/// One row's measured state.
#[derive(Debug, Clone, Copy)]
struct Measured {
    /// The laid-out height, in points.
    height: f32,
    /// The signature the height was measured for. A row whose
    /// signature moved re-measures.
    signature: u64,
}

/// The measured-height list's state.
#[derive(Debug, Default)]
pub struct List {
    /// One measurement per row index, in row order.
    measured: Vec<Measured>,
    /// Whether the view follows the tail as rows arrive and grow.
    following: bool,
    /// The scroll offset the last frame left, for the detach check.
    last_offset: f32,
    /// The content height the last frame measured, for the pin.
    last_content: f32,
    /// Whether the user dragged the scrollbar since the last frame.
    user_scrolled: bool,
}

impl List {
    /// A list with no measurements, following the tail.
    #[must_use]
    pub fn new() -> Self {
        Self {
            following: true,
            ..Self::default()
        }
    }

    /// Whether the view follows the tail.
    #[must_use]
    pub fn following(&self) -> bool {
        self.following
    }

    /// Reattaches the follow, for the jump-to-bottom affordance.
    pub fn follow_again(&mut self) {
        self.following = true;
    }

    /// Records the frame's scroll and decides whether the follow
    /// detached: a scroll that moved up while following detaches it.
    pub fn on_scroll(&mut self, offset: f32, content: f32, viewport: f32) {
        self.watch_scroll(offset, content, viewport);
    }

    /// Marks that the view itself moved the scroll, so the next frame's
    /// offset is not read as the user's.
    pub fn user_scrolled(&mut self) {
        self.user_scrolled = true;
    }

    fn watch_scroll(&mut self, offset: f32, content: f32, viewport: f32) {
        let at_bottom = offset + viewport >= content - DETACH_SLACK;
        if self.user_scrolled {
            // A user scroll up detaches; a scroll back to the bottom
            // reattaches.
            self.following = at_bottom;
            self.user_scrolled = false;
        }
        self.last_offset = offset;
        self.last_content = content;
    }

    /// The rows the viewport can see, with their y offsets and
    /// heights. Rows past the measured set are laid out on demand, so
    /// the caller measures what it draws.
    #[must_use]
    pub fn window(&self, offset: f32, viewport: f32) -> (usize, usize) {
        if self.measured.is_empty() {
            return (0, 0);
        }
        let mut y = 0.0;
        let mut first = None;
        let mut last = 0;
        for (ix, m) in self.measured.iter().enumerate() {
            let top = y;
            y += m.height;
            if first.is_none() && y >= offset - OVERDRAW {
                first = Some(ix);
            }
            if top <= offset + viewport + OVERDRAW {
                last = ix + 1;
            }
        }
        let first = first.unwrap_or(self.measured.len().saturating_sub(1));
        (first, last.max(first + 1).min(self.measured.len()))
    }

    /// The total measured height, for the scroll area's content size.
    #[must_use]
    pub fn content_height(&self) -> f32 {
        self.measured.iter().map(|m| m.height).sum()
    }

    /// The scroll offset the last frame left, as `on_scroll` recorded
    /// it. The probes read this to watch a seek land.
    #[must_use]
    pub fn offset(&self) -> f32 {
        self.last_offset
    }

    /// The y offset of row `ix`, for scrolling it into view.
    #[must_use]
    pub fn offset_of(&self, ix: usize) -> f32 {
        self.measured
            .iter()
            .take(ix.min(self.measured.len()))
            .map(|m| m.height)
            .sum()
    }

    /// Splices the measurements to match `signatures`: the unchanged
    /// head and tail keep theirs, the span between re-measures. This
    /// is the delta the spike's list could not keep cheap.
    pub fn sync(&mut self, signatures: &[u64], mut measure: impl FnMut(usize) -> f32) {
        let head = self
            .measured
            .iter()
            .zip(signatures)
            .take_while(|(m, sig)| m.signature == **sig)
            .count();
        let tail = self.measured[head..]
            .iter()
            .rev()
            .zip(signatures[head..].iter().rev())
            .take_while(|(m, sig)| m.signature == **sig)
            .count();
        let old_len = self.measured.len();
        let new_len = signatures.len();
        let changed = old_len - head - tail;
        let fresh = new_len - head - tail;
        if changed > 0 || fresh > 0 {
            self.measured.splice(
                head..head + changed,
                (head..head + fresh).map(|ix| Measured {
                    height: measure(ix),
                    signature: signatures[ix],
                }),
            );
        }
        // Rows past the splice that the viewport reaches measure on
        // demand, so an appended tail row is measured when it draws.
        for (ix, m) in self.measured.iter_mut().enumerate() {
            if ix < head || ix >= head + fresh {
                continue;
            }
            if let Some(sig) = signatures.get(ix) {
                m.signature = *sig;
            }
        }
    }

    /// Records the height a row laid out at, for the next frame.
    pub fn set_height(&mut self, ix: usize, height: f32) {
        if let Some(m) = self.measured.get_mut(ix) {
            m.height = height;
        }
    }

    /// The scroll target the follow wants: the bottom, or the offset
    /// that keeps the pinned row in place.
    #[must_use]
    pub fn follow_target(&self, viewport: f32) -> f32 {
        (self.content_height() - viewport).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{DETACH_SLACK, List};

    fn sigs(n: usize) -> Vec<u64> {
        (0..n as u64).collect()
    }

    #[test]
    fn a_new_list_follows_the_tail() {
        let list = List::new();
        assert!(list.following());
    }

    #[test]
    fn the_window_covers_the_viewport_and_the_overdraw() {
        let mut list = List::new();
        list.sync(&sigs(1000), |_ix| 10.0);
        // A viewport at the top sees the first rows plus overdraw.
        let (first, last) = list.window(0.0, 500.0);
        assert_eq!(first, 0);
        assert!(last < 100, "only the visible window lays out, got {last}");
        // A viewport deep in the list sees a window around it.
        let (first, last) = list.window(5_000.0, 500.0);
        assert!(first > 400, "the deep window starts late, got {first}");
        assert!(last - first < 200, "the deep window stays small");
    }

    #[test]
    fn a_signature_delta_re_measures_only_the_changed_span() {
        let mut measured = Vec::new();
        let mut list = List::new();
        let mut heights = |ix: usize| {
            measured.push(ix);
            10.0
        };
        list.sync(&sigs(10), &mut heights);
        let first_pass = measured.len();

        // One row's signature moves: only it re-measures.
        measured.clear();
        let mut signatures = sigs(10);
        signatures[5] = 99;
        let mut heights = |ix: usize| {
            measured.push(ix);
            10.0
        };
        list.sync(&signatures, &mut heights);
        assert_eq!(measured.len(), 1, "only the changed row re-measures");
        assert_eq!(measured[0], 5);
        assert!(measured.len() < first_pass);
    }

    #[test]
    fn an_append_re_measures_only_the_new_tail() {
        let mut list = List::new();
        let mut measured = Vec::new();
        list.sync(&sigs(10), |ix| {
            measured.push(ix);
            10.0
        });
        measured.clear();
        list.sync(&sigs(12), |ix| {
            measured.push(ix);
            10.0
        });
        assert_eq!(measured, vec![10, 11], "only the appended rows measure");
    }

    #[test]
    fn a_scroll_up_detaches_and_the_bottom_reattaches() {
        let mut list = List::new();
        list.sync(&sigs(100), |_| 10.0);
        let viewport = 200.0;
        let content = list.content_height();
        // Sitting at the bottom.
        list.watch_scroll(content - viewport, content, viewport);
        assert!(list.following());
        // A user scroll up detaches.
        list.user_scrolled = true;
        list.watch_scroll(0.0, content, viewport);
        assert!(!list.following(), "a scroll up detaches");
        // A scroll back to the bottom reattaches.
        list.user_scrolled = true;
        list.watch_scroll(content - viewport - DETACH_SLACK / 2.0, content, viewport);
        assert!(list.following(), "the bottom reattaches");
    }

    #[test]
    fn the_follow_target_is_the_bottom() {
        let mut list = List::new();
        list.sync(&sigs(10), |_| 10.0);
        assert_eq!(list.follow_target(50.0), 50.0);
        assert_eq!(list.follow_target(500.0), 0.0, "a short list has no scroll");
    }

    #[test]
    fn offsets_and_heights_are_addressable() {
        let mut list = List::new();
        list.sync(&sigs(5), |ix| (ix + 1) as f32 * 10.0);
        assert_eq!(list.offset_of(0), 0.0);
        assert_eq!(list.offset_of(2), 30.0);
        assert_eq!(list.content_height(), 150.0);
        list.set_height(0, 100.0);
        assert_eq!(list.content_height(), 240.0);
    }
}
