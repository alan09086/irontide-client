//! Column model for the torrent list view.
//!
//! Defines column identifiers, per-column metadata (default/min widths),
//! and the [`ColumnConfig`] that tracks the runtime column state (order,
//! visibility, and pixel widths).

use std::collections::HashSet;

// ── ColumnId ────────────────────────────────────────────────────────────────

/// Identifies a single column in the torrent list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId {
    Name,
    Progress,
    State,
    DownRate,
    UpRate,
    Seeds,
    Peers,
    Eta,
    Size,
    Ratio,
}

impl ColumnId {
    /// Parse a column identifier from its canonical lowercase name.
    ///
    /// Returns `None` for unrecognised names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "name" => Some(Self::Name),
            "progress" => Some(Self::Progress),
            "state" => Some(Self::State),
            "down_rate" => Some(Self::DownRate),
            "up_rate" => Some(Self::UpRate),
            "seeds" => Some(Self::Seeds),
            "peers" => Some(Self::Peers),
            "eta" => Some(Self::Eta),
            "size" => Some(Self::Size),
            "ratio" => Some(Self::Ratio),
            _ => None,
        }
    }

    /// Return the canonical lowercase name for this column (inverse of
    /// [`from_name`]).
    pub fn to_name(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Progress => "progress",
            Self::State => "state",
            Self::DownRate => "down_rate",
            Self::UpRate => "up_rate",
            Self::Seeds => "seeds",
            Self::Peers => "peers",
            Self::Eta => "eta",
            Self::Size => "size",
            Self::Ratio => "ratio",
        }
    }

    /// Map a 0-based integer index to the corresponding column.
    ///
    /// Returns `None` for indices outside `0..=9`.
    pub fn from_index(i: i32) -> Option<Self> {
        match i {
            0 => Some(Self::Name),
            1 => Some(Self::Progress),
            2 => Some(Self::State),
            3 => Some(Self::DownRate),
            4 => Some(Self::UpRate),
            5 => Some(Self::Seeds),
            6 => Some(Self::Peers),
            7 => Some(Self::Eta),
            8 => Some(Self::Size),
            9 => Some(Self::Ratio),
            _ => None,
        }
    }

    /// Return the 0-based integer index for this column (inverse of
    /// [`from_index`]).
    pub fn to_index(self) -> i32 {
        match self {
            Self::Name => 0,
            Self::Progress => 1,
            Self::State => 2,
            Self::DownRate => 3,
            Self::UpRate => 4,
            Self::Seeds => 5,
            Self::Peers => 6,
            Self::Eta => 7,
            Self::Size => 8,
            Self::Ratio => 9,
        }
    }

    /// Default pixel width for this column.
    pub fn default_width(self) -> f32 {
        match self {
            Self::Name => 200.0,
            Self::Progress => 120.0,
            Self::State => 90.0,
            Self::DownRate => 80.0,
            Self::UpRate => 80.0,
            Self::Seeds => 50.0,
            Self::Peers => 50.0,
            Self::Eta => 70.0,
            Self::Size => 80.0,
            Self::Ratio => 60.0,
        }
    }

    /// Minimum pixel width for this column.
    #[allow(dead_code)] // Used in M164 drag-resize
    pub fn min_width(self) -> f32 {
        match self {
            Self::Name => 100.0,
            Self::Progress => 60.0,
            Self::State => 60.0,
            Self::DownRate => 50.0,
            Self::UpRate => 50.0,
            Self::Seeds => 35.0,
            Self::Peers => 35.0,
            Self::Eta => 50.0,
            Self::Size => 50.0,
            Self::Ratio => 40.0,
        }
    }
}

// ── DEFAULT_ORDER ────────────────────────────────────────────────────────────

/// The default column display order (all 10 columns).
pub const DEFAULT_ORDER: [ColumnId; 10] = [
    ColumnId::Name,
    ColumnId::Progress,
    ColumnId::State,
    ColumnId::DownRate,
    ColumnId::UpRate,
    ColumnId::Seeds,
    ColumnId::Peers,
    ColumnId::Eta,
    ColumnId::Size,
    ColumnId::Ratio,
];

// ── ColumnConfig ─────────────────────────────────────────────────────────────

/// Runtime column configuration: order, visibility, and per-column widths.
pub struct ColumnConfig {
    /// Ordered list of column identifiers (determines left-to-right display
    /// order, including hidden columns).
    pub order: Vec<ColumnId>,
    /// Set of currently visible columns.
    pub visible: HashSet<ColumnId>,
    /// Pixel widths for each column, in the same order as [`order`].
    pub widths: Vec<f32>,
}

impl Default for ColumnConfig {
    fn default() -> Self {
        let order: Vec<ColumnId> = DEFAULT_ORDER.to_vec();
        let visible: HashSet<ColumnId> = order.iter().copied().collect();
        let widths: Vec<f32> = order.iter().map(|c| c.default_width()).collect();
        Self {
            order,
            visible,
            widths,
        }
    }
}

impl ColumnConfig {
    /// Build a `ColumnConfig` from a saved [`irontide_config::GuiConfig`].
    ///
    /// Falls back to defaults for any missing or unrecognised values.
    pub fn from_gui_config(gui: &irontide_config::GuiConfig) -> Self {
        let mut cfg = Self::default();

        // Restore column order.
        if let Some(names) = &gui.column_order {
            let parsed: Vec<ColumnId> = names
                .iter()
                .filter_map(|n| ColumnId::from_name(n))
                .collect();
            if !parsed.is_empty() {
                // Ensure every known column is present (append any that are
                // missing from the saved order so we never lose a column).
                let mut full_order = parsed.clone();
                for col in DEFAULT_ORDER {
                    if !full_order.contains(&col) {
                        full_order.push(col);
                    }
                }
                cfg.order = full_order;
                // Rebuild widths to match the new order (use defaults for now;
                // saved widths are applied below).
                cfg.widths = cfg.order.iter().map(|c| c.default_width()).collect();
            }
        }

        // Restore visibility.
        if let Some(vis_names) = &gui.column_visibility {
            let visible: HashSet<ColumnId> = vis_names
                .iter()
                .filter_map(|n| ColumnId::from_name(n))
                .collect();
            // Name must always be visible.
            let mut visible = visible;
            visible.insert(ColumnId::Name);
            cfg.visible = visible;
        }

        // Restore per-column widths.  The widths vector in GuiConfig is
        // parallel to column_order, so we only apply it when both lengths
        // match the restored order.
        if let Some(saved_widths) = &gui.column_widths {
            let order_len = cfg.order.len();
            if saved_widths.len() == order_len {
                cfg.widths = saved_widths.clone();
            } else if let Some(saved_order_names) = &gui.column_order {
                // Lengths differ — try a best-effort column-by-column match.
                let saved_order: Vec<ColumnId> = saved_order_names
                    .iter()
                    .filter_map(|n| ColumnId::from_name(n))
                    .collect();
                for (i, col) in cfg.order.iter().enumerate() {
                    if let Some(pos) = saved_order.iter().position(|c| c == col)
                        && let Some(&w) = saved_widths.get(pos)
                    {
                        cfg.widths[i] = w;
                    }
                }
            }
        }

        cfg
    }

    /// Serialise this `ColumnConfig` into a [`irontide_config::GuiConfig`]
    /// suitable for persistence.
    pub fn to_gui_config(&self) -> irontide_config::GuiConfig {
        let column_order: Vec<String> = self.order.iter().map(|c| c.to_name().to_owned()).collect();
        let column_visibility: Vec<String> = self
            .order
            .iter()
            .filter(|c| self.visible.contains(c))
            .map(|c| c.to_name().to_owned())
            .collect();
        let column_widths: Vec<f32> = self.widths.clone();

        irontide_config::GuiConfig {
            column_order: Some(column_order),
            column_visibility: Some(column_visibility),
            column_widths: Some(column_widths),
        }
    }

    /// Drag-reorder: remove the column at position `from` and insert it at
    /// position `to`.  Both indices must be within bounds; out-of-bounds
    /// indices are silently ignored.
    #[allow(dead_code)] // Used in M164 drag-reorder
    pub fn move_column(&mut self, from: usize, to: usize) {
        if from >= self.order.len() || to >= self.order.len() || from == to {
            return;
        }
        let col = self.order.remove(from);
        let width = self.widths.remove(from);
        // After the removal, `to` may refer to a shifted position.
        let insert_at = to.min(self.order.len());
        self.order.insert(insert_at, col);
        self.widths.insert(insert_at, width);
    }

    /// Toggle the visibility of `col`.
    ///
    /// [`ColumnId::Name`] can never be hidden — calling this with `Name`
    /// while it is visible is a no-op.
    #[allow(dead_code)] // Used in M164 column visibility menu
    pub fn toggle_visibility(&mut self, col: ColumnId) {
        if col == ColumnId::Name && self.visible.contains(&col) {
            // Name must always remain visible.
            return;
        }
        if self.visible.contains(&col) {
            self.visible.remove(&col);
        } else {
            self.visible.insert(col);
        }
    }
}

// ── SortState ────────────────────────────────────────────────────────────────

/// Current sort column and direction for the torrent list.
#[derive(Debug, Clone, Copy)]
pub struct SortState {
    pub column: ColumnId,
    pub ascending: bool,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            column: ColumnId::Name,
            ascending: true,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// All 10 `ColumnId` variants survive a `to_name` → `from_name` round-trip.
    #[test]
    fn test_column_id_name_round_trip() {
        let all = [
            ColumnId::Name,
            ColumnId::Progress,
            ColumnId::State,
            ColumnId::DownRate,
            ColumnId::UpRate,
            ColumnId::Seeds,
            ColumnId::Peers,
            ColumnId::Eta,
            ColumnId::Size,
            ColumnId::Ratio,
        ];
        for col in all {
            let name = col.to_name();
            let parsed = ColumnId::from_name(name)
                .unwrap_or_else(|| panic!("from_name failed for {name:?}"));
            assert_eq!(parsed, col, "round-trip failed for {name:?}");
        }
    }

    /// `DEFAULT_ORDER` contains exactly 10 elements.
    #[test]
    fn test_default_order_count() {
        assert_eq!(DEFAULT_ORDER.len(), 10);
    }

    /// Toggling the `Name` column's visibility while it is visible is a no-op.
    #[test]
    fn test_name_always_visible() {
        let mut cfg = ColumnConfig::default();
        assert!(
            cfg.visible.contains(&ColumnId::Name),
            "Name should start visible"
        );
        cfg.toggle_visibility(ColumnId::Name);
        assert!(
            cfg.visible.contains(&ColumnId::Name),
            "Name should still be visible after toggle attempt"
        );
    }

    /// Moving a column from index 0 to index 2 reorders the `order` vector
    /// correctly.
    #[test]
    fn test_column_reorder() {
        let mut cfg = ColumnConfig::default();
        // Initial: [Name, Progress, State, DownRate, UpRate, Seeds, Peers, Eta, Size, Ratio]
        let original_at_1 = cfg.order[1];
        let original_at_2 = cfg.order[2];
        let moved_col = cfg.order[0]; // Name

        cfg.move_column(0, 2);

        // After moving index 0 to index 2:
        // [Progress, State, Name, DownRate, UpRate, Seeds, Peers, Eta, Size, Ratio]
        assert_eq!(
            cfg.order[0], original_at_1,
            "index 0 should now hold old index 1"
        );
        assert_eq!(
            cfg.order[1], original_at_2,
            "index 1 should now hold old index 2"
        );
        assert_eq!(
            cfg.order[2], moved_col,
            "index 2 should hold the moved column"
        );
        assert_eq!(cfg.order.len(), 10, "total column count must be unchanged");
        assert_eq!(
            cfg.widths.len(),
            10,
            "widths length must match order length"
        );
    }

    /// Serialise to `GuiConfig`, deserialise back, verify order/visibility/widths.
    #[test]
    fn test_column_config_persistence() {
        let mut original = ColumnConfig::default();
        // Hide a couple of columns to make the test non-trivial.
        original.toggle_visibility(ColumnId::Ratio);
        original.toggle_visibility(ColumnId::Eta);
        // Adjust a width.
        if let Some(w) = original.widths.first_mut() {
            *w = 250.0;
        }

        let gui = original.to_gui_config();
        let restored = ColumnConfig::from_gui_config(&gui);

        assert_eq!(
            restored.order, original.order,
            "order must survive round-trip"
        );
        assert_eq!(
            restored.visible, original.visible,
            "visibility must survive round-trip"
        );
        assert_eq!(
            restored.widths, original.widths,
            "widths must survive round-trip"
        );
    }
}
