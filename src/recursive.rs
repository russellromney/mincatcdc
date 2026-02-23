use std::cell::Cell;
use std::marker::PhantomData;

use slotmap::{Key, SlotMap, new_key_type};

use crate::Chunk;

new_key_type! {
    pub struct CartNodeKey;
}

pub type CartNodes<T> = SlotMap<CartNodeKey, CartNode<T>>;

#[derive(Clone)]
pub struct CartNode<T> {
    value: T,
    // Heap invariant: left/right are larger than this node, and respectively
    // come before/after it.
    left: CartNodeKey,
    right: CartNodeKey,
    parent: CartNodeKey,
    // Previous/next node in the sequence.
    prev: CartNodeKey,
    next: CartNodeKey,
}

#[derive(Clone)]
pub struct CartTree<T> {
    root: CartNodeKey,
    first: CartNodeKey,
    last: CartNodeKey,
    phantom: PhantomData<T>,
}

impl<T> Default for CartTree<T> {
    fn default() -> Self {
        Self {
            root: CartNodeKey::null(),
            first: CartNodeKey::null(),
            last: CartNodeKey::null(),
            phantom: PhantomData,
        }
    }
}

impl<T> CartTree<T> {
    pub fn is_empty(&self) -> bool {
        self.root.is_null()
    }

    pub fn push_front(
        &mut self,
        value: T,
        nodes: &mut CartNodes<T>,
        is_less: impl Fn(&T, &T) -> bool,
    ) {
        let mut parent = self.first;
        let mut right = CartNodeKey::null();
        while !parent.is_null() && is_less(&value, &nodes[parent].value) {
            right = parent;
            parent = nodes[parent].parent;
        }

        let node = nodes.insert(CartNode {
            value,
            left: CartNodeKey::null(),
            right,
            parent,
            prev: CartNodeKey::null(),
            next: self.first,
        });

        if let Some(r) = nodes.get_mut(right) {
            r.parent = node;
        }

        if let Some(p) = nodes.get_mut(parent) {
            p.left = node;
        } else {
            self.root = node;
        }

        if let Some(l) = nodes.get_mut(self.first) {
            l.prev = node;
        } else {
            self.last = node;
        }
        self.first = node;
    }

    pub fn pop_front(&mut self, nodes: &mut CartNodes<T>) -> Option<T> {
        let node = nodes.remove(self.first)?;
        self.first = node.next;

        if let Some(n) = nodes.get_mut(node.next) {
            n.prev = CartNodeKey::null();
        } else {
            self.last = CartNodeKey::null();
        };

        if let Some(p) = nodes.get_mut(node.parent) {
            p.left = node.right;
        } else {
            self.root = node.right;
        };

        if let Some(r) = nodes.get_mut(node.right) {
            r.parent = node.parent;
        }

        Some(node.value)
    }

    pub fn push_back(
        &mut self,
        value: T,
        nodes: &mut CartNodes<T>,
        is_less: impl Fn(&T, &T) -> bool,
    ) {
        let mut parent = self.last;
        let mut left = CartNodeKey::null();
        while !parent.is_null() && is_less(&value, &nodes[parent].value) {
            left = parent;
            parent = nodes[parent].parent;
        }

        let node = nodes.insert(CartNode {
            value,
            left,
            right: CartNodeKey::null(),
            parent,
            prev: self.last,
            next: CartNodeKey::null(),
        });

        if let Some(l) = nodes.get_mut(left) {
            l.parent = node;
        }
        if let Some(p) = nodes.get_mut(parent) {
            p.right = node;
        } else {
            self.root = node;
        }
        if let Some(l) = nodes.get_mut(self.last) {
            l.next = node;
        } else {
            self.first = node;
        }
        self.last = node;
    }

    pub fn pop_back(&mut self, nodes: &mut CartNodes<T>) -> Option<T> {
        let node = nodes.remove(self.last)?;
        self.last = node.prev;

        if let Some(n) = nodes.get_mut(node.prev) {
            n.next = CartNodeKey::null();
        } else {
            self.first = CartNodeKey::null();
        };

        if let Some(p) = nodes.get_mut(node.parent) {
            p.right = node.left;
        } else {
            self.root = node.left;
        };

        if let Some(l) = nodes.get_mut(node.left) {
            l.parent = node.parent;
        }

        Some(node.value)
    }

    pub fn min<'a>(&self, nodes: &'a CartNodes<T>) -> Option<&'a T> {
        nodes.get(self.root).map(|n| &n.value)
    }

    pub fn first<'a>(&self, nodes: &'a CartNodes<T>) -> Option<&'a T> {
        nodes.get(self.first).map(|n| &n.value)
    }

    pub fn last<'a>(&self, nodes: &'a CartNodes<T>) -> Option<&'a T> {
        nodes.get(self.last).map(|n| &n.value)
    }

    pub fn split_min(self, nodes: &mut CartNodes<T>) -> Option<(Self, T, Self)> {
        let m = nodes.remove(self.root)?;
        let left = if m.left.is_null() {
            Self::default()
        } else {
            Self {
                root: m.left,
                first: self.first,
                last: m.prev,
                phantom: PhantomData,
            }
        };
        let right = if m.right.is_null() {
            Self::default()
        } else {
            Self {
                root: m.right,
                first: m.next,
                last: self.last,
                phantom: PhantomData,
            }
        };
        if let Some(ll) = nodes.get_mut(left.last) {
            ll.next = CartNodeKey::null()
        }
        if let Some(rf) = nodes.get_mut(right.first) {
            rf.prev = CartNodeKey::null()
        }
        if let Some(lm) = nodes.get_mut(left.root) {
            lm.parent = CartNodeKey::null();
        }
        if let Some(rm) = nodes.get_mut(right.root) {
            rm.parent = CartNodeKey::null();
        }
        Some((left, m.value, right))
    }
    
    pub fn len(&self, nodes: &CartNodes<T>) -> usize {
        let mut cur = self.first;
        let mut l = 0;
        while let Some(n) = nodes.get(cur) {
            cur = n.next;
            l += 1;
        }
        l
    }
}

#[derive(Clone)]
struct MinChunk {
    start: usize,
    stop: usize,
    eval: Cell<Option<u32>>,
    eval_lower_bound: Cell<Option<u32>>,
    argmin: Cell<Option<usize>>,
}

impl MinChunk {
    fn new(start: usize, stop: usize) -> Self {
        Self {
            start,
            stop,
            eval: Cell::new(None),
            eval_lower_bound: Cell::new(None),
            argmin: Cell::new(None),
        }
    }

    fn clamp(mut self, start: usize, stop: usize) -> Option<Self> {
        let new_start = start.clamp(self.start, self.stop);
        let new_stop = stop.clamp(self.start, self.stop);
        if new_start >= new_stop {
            return None;
        }
        self.start = new_start;
        self.stop = new_stop;
        self.eval.set(None);
        self.argmin.set(None);
        Some(self)
    }

    fn is_less(&self, other: &Self, bytes: &[u8]) -> bool {
        match (self.eval.get(), other.eval.get()) {
            (Some(a), Some(b)) => a < b,
            (Some(a), None) => {
                if let Some(bl) = other.eval_lower_bound.get() {
                    if a < bl {
                        return true;
                    }
                }
                a < other.eval(bytes)
            },
            (None, Some(b)) => {
                if let Some(al) = self.eval_lower_bound.get() {
                    if !(al < b) {
                        return false;
                    }
                }
                self.eval(bytes) < b
            },
            (None, None) => self.eval(bytes) < other.eval(bytes),
        }
    }
    
    fn argmin(&self, bytes: &[u8]) -> usize {
        self.eval(bytes);
        return self.argmin.get().unwrap();
    }

    fn eval(&self, bytes: &[u8]) -> u32 {
        if let Some(h) = self.eval.get() {
            return h;
        }

        let (argmin, hash) = crate::simd::argmin_u32_overlapping_hashed::<true>(
            &bytes[self.start..self.stop],
            crate::DEFAULT_MULTIPLIER,
            crate::DEFAULT_ADDEND,
        );
        self.eval.set(Some(hash));
        self.argmin.set(Some(self.start + argmin + 4));
        hash
    }
}

#[derive(Clone)]
pub struct SliceRecMinCdcHash4<'a> {
    min_size: usize,
    max_size: usize,
    horizon: usize,
    bytes: &'a [u8],
    out_offset: usize,
    scan_offset: usize,
    nodes: CartNodes<MinChunk>,
    tree: CartTree<MinChunk>,
}

impl<'a> SliceRecMinCdcHash4<'a> {
    pub fn new(bytes: &'a [u8], min_size: usize, max_size: usize, horizon: usize) -> Self {
        assert!(max_size >= 2 * min_size);
        assert!(horizon >= max_size);
        Self {
            min_size,
            max_size,
            horizon,
            bytes,
            out_offset: 0,
            scan_offset: 0,
            nodes: CartNodes::default(),
            tree: CartTree::default(),
        }
    }

    pub fn chunks(&mut self) -> Vec<Chunk<'a>> {
        let mut chunks = Vec::new();
        while self.out_offset < self.bytes.len() {
            // Last chunk.
            if self.out_offset + self.max_size >= self.bytes.len() {
                chunks.push(Chunk::new(&self.bytes[self.out_offset..], self.out_offset));
                self.out_offset = self.bytes.len();
                return chunks;
            }

            // Add to tree to fill horizon.
            let max_read = (self.out_offset + self.horizon).min(self.bytes.len());
            while max_read > self.scan_offset {
                let chunk_size = (max_read - self.scan_offset).min(1024);
                let min_chunk = MinChunk::new(self.scan_offset, self.scan_offset + chunk_size);
                self.tree
                    .push_back(min_chunk, &mut self.nodes, |a, b| a.is_less(b, self.bytes));
                self.scan_offset += chunk_size;
            }

            // Split out chunks.
            let mut tree = core::mem::take(&mut self.tree);
            self.trim_tree(&mut tree, self.out_offset, self.bytes.len());
            let (left, chunk, right) = tree.split_min(&mut self.nodes).unwrap();
            let splitpoint = chunk.argmin(&self.bytes);
            let prev_offset = if splitpoint - self.out_offset > self.max_size {
                self.split_recursive(left, self.out_offset, splitpoint, &mut chunks)
            } else {
                self.out_offset
            };
            chunks.push(Chunk::new(
                &self.bytes[prev_offset..splitpoint],
                prev_offset,
            ));
            self.out_offset = splitpoint;
            self.tree = right;
        }
        chunks
    }
    
    fn trim_tree(
        &mut self,
        tree: &mut CartTree<MinChunk>,
        lower_bound: usize,
        upper_bound: usize,
    ) {
        // Trim tree to permissable splitpoints.
        let lo_split = lower_bound + self.min_size;
        let hi_split = upper_bound - self.min_size;
        assert!(lo_split < hi_split);
        while tree.first(&self.nodes).is_some_and(|n| n.start < lo_split) {
            let chunk = tree.pop_front(&mut self.nodes).unwrap();
            if let Some(clamped) = chunk.clamp(lo_split, hi_split) {
                tree.push_front(clamped, &mut self.nodes, |a, b| a.is_less(b, self.bytes));
                break;
            }
        }

        while tree.last(&self.nodes).is_some_and(|n| n.stop > hi_split) {
            let chunk = tree.pop_back(&mut self.nodes).unwrap();
            if let Some(clamped) = chunk.clamp(lo_split, hi_split) {
                tree.push_back(clamped, &mut self.nodes, |a, b| a.is_less(b, self.bytes));
                break;
            }
        }
    }

    fn split_recursive(
        &mut self,
        mut tree: CartTree<MinChunk>,
        mut lower_bound: usize,
        upper_bound: usize,
        splits: &mut Vec<Chunk<'a>>,
    ) -> usize {
        loop {
            self.trim_tree(&mut tree, lower_bound, upper_bound);
            let (left, chunk, right) = tree.split_min(&mut self.nodes).unwrap();
            let splitpoint = chunk.argmin(&self.bytes);
            let prev_offset = if splitpoint - lower_bound > self.max_size {
                self.split_recursive(left, lower_bound, splitpoint, splits)
            } else {
                lower_bound
            };
            splits.push(Chunk::new(
                &self.bytes[prev_offset..splitpoint],
                prev_offset,
            ));
            if upper_bound - splitpoint > self.max_size {
                tree = right;
                lower_bound = splitpoint;
            } else {
                return splitpoint;
            }
        }
    }
}
