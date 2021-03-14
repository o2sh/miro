use crate::core::cell::{Cell, CellAttributes};

#[derive(Debug, Clone)]
pub struct CellCluster {
    pub attrs: CellAttributes,
    pub text: String,
    pub byte_to_cell_idx: Vec<usize>,
}

impl CellCluster {
    pub fn make_cluster<'a>(iter: impl Iterator<Item = (usize, &'a Cell)>) -> Vec<CellCluster> {
        let mut last_cluster = None;
        let mut clusters = Vec::new();

        for (cell_idx, c) in iter {
            let cell_str = c.str();

            last_cluster = match last_cluster.take() {
                None => Some(CellCluster::new(c.attrs().clone(), cell_str, cell_idx)),
                Some(mut last) => {
                    if last.attrs != *c.attrs() {
                        clusters.push(last);
                        Some(CellCluster::new(c.attrs().clone(), cell_str, cell_idx))
                    } else {
                        last.add(cell_str, cell_idx);
                        Some(last)
                    }
                }
            };
        }

        if let Some(cluster) = last_cluster {
            clusters.push(cluster);
        }

        clusters
    }

    fn new(attrs: CellAttributes, text: &str, cell_idx: usize) -> CellCluster {
        let mut idx = Vec::new();
        for _ in 0..text.len() {
            idx.push(cell_idx);
        }
        CellCluster { attrs, text: text.into(), byte_to_cell_idx: idx }
    }

    fn add(&mut self, text: &str, cell_idx: usize) {
        for _ in 0..text.len() {
            self.byte_to_cell_idx.push(cell_idx);
        }
        self.text.push_str(text);
    }
}
