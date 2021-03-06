use super::filetree::{
    FileTreeItem, FileTreeItemKind, FileTreeItems, PathCollapsed,
};
use asyncgit::StatusItem;
use std::{cmp, collections::BTreeSet};

///
#[derive(Default)]
pub struct StatusTree {
    pub tree: FileTreeItems,
    pub selection: Option<usize>,
}

///
#[derive(Copy, Clone, Debug)]
pub enum MoveSelection {
    Up,
    Down,
    Left,
    Right,
}

struct SelectionChange {
    new_index: usize,
    changes: bool,
}
impl SelectionChange {
    fn new(new_index: usize, changes: bool) -> Self {
        Self { new_index, changes }
    }
}

impl StatusTree {
    /// update tree with a new list, try to retain selection and collapse states
    pub fn update(&mut self, list: &[StatusItem]) {
        let last_collapsed = self.all_collapsed();

        let last_selection =
            self.selected_item().map(|e| e.info.full_path);
        let last_selection_index = self.selection.unwrap_or(0);

        self.tree = FileTreeItems::new(list, &last_collapsed);
        self.selection =
            if let Some(ref last_selection) = last_selection {
                self.find_last_selection(
                    last_selection,
                    last_selection_index,
                )
                .or_else(|| self.tree.items().first().map(|_| 0))
            } else {
                // simply select first
                self.tree.items().first().map(|_| 0)
            };

        self.update_visibility(None, 0, true);
    }

    ///
    pub fn move_selection(&mut self, dir: MoveSelection) -> bool {
        if let Some(selection) = self.selection {
            let selection_change = match dir {
                MoveSelection::Up => {
                    self.selection_updown(selection, true)
                }
                MoveSelection::Down => {
                    self.selection_updown(selection, false)
                }

                MoveSelection::Left => self.selection_left(selection),
                MoveSelection::Right => {
                    self.selection_right(selection)
                }
            };

            let changed = selection_change.new_index != selection;

            self.selection = Some(selection_change.new_index);

            changed || selection_change.changes
        } else {
            false
        }
    }

    ///
    pub fn selected_item(&self) -> Option<FileTreeItem> {
        self.selection.map(|i| self.tree[i].clone())
    }

    ///
    pub fn is_empty(&self) -> bool {
        self.tree.items().is_empty()
    }

    fn all_collapsed(&self) -> BTreeSet<&String> {
        let mut res = BTreeSet::new();

        for i in self.tree.items() {
            if let FileTreeItemKind::Path(PathCollapsed(collapsed)) =
                i.kind
            {
                if collapsed {
                    res.insert(&i.info.full_path);
                }
            }
        }

        res
    }

    fn find_last_selection(
        &self,
        last_selection: &str,
        last_index: usize,
    ) -> Option<usize> {
        if self.is_empty() {
            return None;
        }

        if let Ok(i) = self.tree.items().binary_search_by(|e| {
            e.info.full_path.as_str().cmp(last_selection)
        }) {
            return Some(i);
        }

        Some(cmp::min(last_index, self.tree.len() - 1))
    }

    fn selection_updown(
        &self,
        current_index: usize,
        up: bool,
    ) -> SelectionChange {
        let mut new_index = current_index;

        let items_max = self.tree.len().saturating_sub(1);

        loop {
            new_index = if up {
                new_index.saturating_sub(1)
            } else {
                new_index.saturating_add(1)
            };

            new_index = cmp::min(new_index, items_max);

            if self.is_visible_index(new_index) {
                break;
            }

            if new_index == 0 || new_index == items_max {
                // limit reached, dont update
                new_index = current_index;
                break;
            }
        }

        SelectionChange::new(new_index, false)
    }

    fn is_visible_index(&self, idx: usize) -> bool {
        self.tree[idx].info.visible
    }

    fn selection_right(
        &mut self,
        current_selection: usize,
    ) -> SelectionChange {
        let item_kind = self.tree[current_selection].kind.clone();
        let item_path =
            self.tree[current_selection].info.full_path.clone();

        if matches!(item_kind,  FileTreeItemKind::Path(PathCollapsed(collapsed))
        if collapsed)
        {
            self.expand(&item_path, current_selection);
            return SelectionChange::new(current_selection, true);
        }

        SelectionChange::new(current_selection, false)
    }

    fn selection_left(
        &mut self,
        current_selection: usize,
    ) -> SelectionChange {
        let item_kind = self.tree[current_selection].kind.clone();
        let item_path =
            self.tree[current_selection].info.full_path.clone();

        if matches!(item_kind, FileTreeItemKind::File(_))
            || matches!(item_kind,FileTreeItemKind::Path(PathCollapsed(collapsed))
        if collapsed)
        {
            SelectionChange::new(
                self.tree
                    .find_parent_index(&item_path, current_selection),
                false,
            )
        } else if matches!(item_kind,  FileTreeItemKind::Path(PathCollapsed(collapsed))
        if !collapsed)
        {
            self.collapse(&item_path, current_selection);
            SelectionChange::new(current_selection, true)
        } else {
            SelectionChange::new(current_selection, false)
        }
    }

    fn collapse(&mut self, path: &str, index: usize) {
        if let FileTreeItemKind::Path(PathCollapsed(
            ref mut collapsed,
        )) = self.tree[index].kind
        {
            *collapsed = true;
        }

        let path = format!("{}/", path);

        for i in index + 1..self.tree.len() {
            let item = &mut self.tree[i];
            let item_path = &item.info.full_path;
            if item_path.starts_with(&path) {
                item.info.visible = false
            } else {
                return;
            }
        }
    }

    fn expand(&mut self, path: &str, current_index: usize) {
        if let FileTreeItemKind::Path(PathCollapsed(
            ref mut collapsed,
        )) = self.tree[current_index].kind
        {
            *collapsed = false;
        }

        let path = format!("{}/", path);

        self.update_visibility(
            Some(path.as_str()),
            current_index + 1,
            false,
        );
    }

    fn update_visibility(
        &mut self,
        prefix: Option<&str>,
        start_idx: usize,
        set_defaults: bool,
    ) {
        // if we are in any subpath that is collapsed we keep skipping over it
        let mut inner_collapsed: Option<String> = None;

        for i in start_idx..self.tree.len() {
            if let Some(ref collapsed_path) = inner_collapsed {
                let p: &String = &self.tree[i].info.full_path;
                if p.starts_with(collapsed_path) {
                    if set_defaults {
                        self.tree[i].info.visible = false;
                    }
                    // we are still in a collapsed inner path
                    continue;
                } else {
                    inner_collapsed = None;
                }
            }

            let item_kind = self.tree[i].kind.clone();
            let item_path = &self.tree[i].info.full_path;

            if matches!(item_kind, FileTreeItemKind::Path(PathCollapsed(collapsed)) if collapsed)
            {
                // we encountered an inner path that is still collapsed
                inner_collapsed = Some(format!("{}/", &item_path));
            }

            if prefix.is_none()
                || item_path.starts_with(prefix.unwrap())
            {
                self.tree[i].info.visible = true
            } else {
                // if we do not set defaults we can early out
                if set_defaults {
                    self.tree[i].info.visible = false;
                } else {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string_vec_to_status(items: &[&str]) -> Vec<StatusItem> {
        items
            .iter()
            .map(|a| StatusItem {
                path: String::from(*a),
                status: None,
            })
            .collect::<Vec<_>>()
    }

    fn get_visibles(tree: &StatusTree) -> Vec<bool> {
        tree.tree
            .items()
            .iter()
            .map(|e| e.info.visible)
            .collect::<Vec<_>>()
    }

    #[test]
    fn test_selection() {
        let items = string_vec_to_status(&[
            "a/b", //
        ]);

        let mut res = StatusTree::default();
        res.update(&items);

        assert!(res.move_selection(MoveSelection::Down));

        assert_eq!(res.selection, Some(1));

        assert!(res.move_selection(MoveSelection::Left));

        assert_eq!(res.selection, Some(0));
    }

    #[test]
    fn test_keep_selected_item() {
        let mut res = StatusTree::default();
        res.update(&string_vec_to_status(&["b"]));

        assert_eq!(res.selection, Some(0));

        res.update(&string_vec_to_status(&["a", "b"]));

        assert_eq!(res.selection, Some(1));
    }

    #[test]
    fn test_keep_selected_index() {
        let mut res = StatusTree::default();
        res.update(&string_vec_to_status(&["a", "b"]));
        res.selection = Some(1);

        res.update(&string_vec_to_status(&["d", "c", "a"]));
        assert_eq!(res.selection, Some(1));
    }

    #[test]
    fn test_keep_collapsed_states() {
        let mut res = StatusTree::default();
        res.update(&string_vec_to_status(&[
            "a/b", //
            "c",
        ]));

        res.collapse("a", 0);

        assert_eq!(
            res.all_collapsed().iter().collect::<Vec<_>>(),
            vec![&&String::from("a")]
        );

        assert_eq!(
            get_visibles(&res),
            vec![
                true,  //
                false, //
                true,  //
            ]
        );

        res.update(&string_vec_to_status(&[
            "a/b", //
            "c",   //
            "d",
        ]));

        assert_eq!(
            res.all_collapsed().iter().collect::<Vec<_>>(),
            vec![&&String::from("a")]
        );

        assert_eq!(
            get_visibles(&res),
            vec![
                true,  //
                false, //
                true,  //
                true
            ]
        );
    }

    #[test]
    fn test_expand() {
        let items = string_vec_to_status(&[
            "a/b/c", //
            "a/d",   //
        ]);

        //0 a/
        //1   b/
        //2     c
        //3   d

        let mut res = StatusTree::default();
        res.update(&items);

        res.collapse(&String::from("a/b"), 1);

        let visibles = get_visibles(&res);

        assert_eq!(
            visibles,
            vec![
                true,  //
                true,  //
                false, //
                true,
            ]
        );

        res.expand(&String::from("a/b"), 1);

        let visibles = get_visibles(&res);

        assert_eq!(
            visibles,
            vec![
                true, //
                true, //
                true, //
                true,
            ]
        );
    }

    #[test]
    fn test_expand_bug() {
        let items = string_vec_to_status(&[
            "a/b/c",  //
            "a/b2/d", //
        ]);

        //0 a/
        //1   b/
        //2     c
        //3   b2/
        //4     d

        let mut res = StatusTree::default();
        res.update(&items);

        res.collapse(&String::from("b"), 1);
        res.collapse(&String::from("a"), 0);

        assert_eq!(
            get_visibles(&res),
            vec![
                true,  //
                false, //
                false, //
                false, //
                false,
            ]
        );

        res.expand(&String::from("a"), 0);

        assert_eq!(
            get_visibles(&res),
            vec![
                true,  //
                true,  //
                false, //
                true,  //
                true,
            ]
        );
    }

    #[test]
    fn test_collapse_too_much() {
        let items = string_vec_to_status(&[
            "a/b",  //
            "a2/c", //
        ]);

        //0 a/
        //1   b
        //2 a2/
        //3   c

        let mut res = StatusTree::default();
        res.update(&items);

        res.collapse(&String::from("a"), 0);

        let visibles = get_visibles(&res);

        assert_eq!(
            visibles,
            vec![
                true,  //
                false, //
                true,  //
                true,
            ]
        );
    }

    #[test]
    fn test_expand_with_collapsed_sub_parts() {
        let items = string_vec_to_status(&[
            "a/b/c", //
            "a/d",   //
        ]);

        //0 a/
        //1   b/
        //2     c
        //3   d

        let mut res = StatusTree::default();
        res.update(&items);

        res.collapse(&String::from("a/b"), 1);

        let visibles = get_visibles(&res);

        assert_eq!(
            visibles,
            vec![
                true,  //
                true,  //
                false, //
                true,
            ]
        );

        res.collapse(&String::from("a"), 0);

        let visibles = get_visibles(&res);

        assert_eq!(
            visibles,
            vec![
                true,  //
                false, //
                false, //
                false,
            ]
        );

        res.expand(&String::from("a"), 0);

        let visibles = get_visibles(&res);

        assert_eq!(
            visibles,
            vec![
                true,  //
                true,  //
                false, //
                true,
            ]
        );
    }

    #[test]
    fn test_selection_skips_collapsed() {
        let items = string_vec_to_status(&[
            "a/b/c", //
            "a/d",   //
        ]);

        //0 a/
        //1   b/
        //2     c
        //3   d

        let mut res = StatusTree::default();
        res.update(&items);
        res.collapse(&String::from("a/b"), 1);
        res.selection = Some(1);

        assert!(res.move_selection(MoveSelection::Down));

        assert_eq!(res.selection, Some(3));
    }
}
