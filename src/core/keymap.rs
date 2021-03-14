use std::fmt::Debug;

#[derive(Debug, Clone)]
struct Node<Value: Debug> {
    label: u8,
    children: Vec<Node<Value>>,
    value: Option<Value>,
}

impl<Value: Debug> Node<Value> {
    fn new(label: u8) -> Self {
        Self { label, children: Vec::new(), value: None }
    }

    fn insert(&mut self, key: &[u8], value: Value) {
        if key.is_empty() {
            self.value = Some(value);
            return;
        }
        match self.children.binary_search_by(|node| node.label.cmp(&key[0])) {
            Ok(idx) => {
                self.children[idx].insert(&key[1..], value);
            }
            Err(idx) => {
                self.children.insert(idx, Node::new(key[0]));
                self.children[idx].insert(&key[1..], value);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyMap<Value: Debug + Clone> {
    root: Node<Value>,
}

impl<Value: Debug + Clone> Default for KeyMap<Value> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Value: Debug + Clone> KeyMap<Value> {
    pub fn new() -> Self {
        Self { root: Node::new(0) }
    }

    pub fn insert<K: AsRef<[u8]>>(&mut self, key: K, value: Value) {
        self.root.insert(key.as_ref(), value)
    }
}
