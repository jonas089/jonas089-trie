use store::{
    db::InMemoryDB,
    types::{Branch, Hashable, Key, Leaf, Node, Root},
};

pub mod merkle;
pub mod store;

pub fn insert_leaf(db: &mut InMemoryDB, new_leaf: &mut Leaf, root_node: Node) -> Root {
    assert_eq!(new_leaf.key.len(), 256);
    let (modified_nodes, new_root) = traverse_trie(db, new_leaf, root_node, false);
    let mut new_root = update_modified_leafs(db, modified_nodes, new_root);
    new_root.hash_and_store(db);
    new_root
}

pub fn update_leaf(db: &mut InMemoryDB, new_leaf: &mut Leaf, root_node: Node) -> Root {
    let (modified_nodes, new_root) = traverse_trie(db, new_leaf, root_node, true);
    let mut new_root = update_modified_leafs(db, modified_nodes, new_root);
    new_root.hash_and_store(db);
    new_root
}

fn traverse_trie(
    db: &mut InMemoryDB,
    new_leaf: &mut Leaf,
    root_node: Node,
    update: bool,
) -> (Vec<(u8, Node)>, Root) {
    let mut new_root: Root = Root::empty();
    let mut modified_nodes: Vec<(u8, Node)> = Vec::new();
    let mut current_node: Node = root_node.clone();
    let mut current_node_pos: u8 = 0;
    let mut idx = 0;
    while idx < new_leaf.key.len() {
        let digit: u8 = new_leaf.key[idx]; // 0 or 1
        assert!(digit == 0 || digit == 1);
        match &mut current_node {
            Node::Root(root) => {
                if digit == 0 {
                    match root.left.clone() {
                        Some(node_hash) => {
                            let root_unwrapped: Root = root_node.clone().unwrap_as_root();
                            modified_nodes.push((0, Node::Root(root_unwrapped)));
                            current_node = db.get(&node_hash).unwrap().clone();
                        }
                        None => {
                            let mut root = current_node.clone().unwrap_as_root();
                            root.left = Some(new_leaf.hash.clone().unwrap());
                            new_leaf.store(db);
                            new_root = root.clone();
                            modified_nodes.push((0, Node::Root(root)));
                            break;
                        }
                    }
                } else {
                    match root.right.clone() {
                        Some(node_hash) => {
                            let root_unwrapped: Root = root_node.clone().unwrap_as_root();
                            modified_nodes.push((0, Node::Root(root_unwrapped)));
                            current_node = db.get(&node_hash).unwrap().clone();
                        }
                        None => {
                            let mut root = current_node.clone().unwrap_as_root();
                            root.right = Some(new_leaf.hash.clone().unwrap());
                            new_leaf.store(db);
                            new_root = root.clone();
                            modified_nodes.push((1, Node::Root(root)));
                            break;
                        }
                    }
                }
            }
            Node::Branch(branch) => {
                idx += branch.key.len();
                if digit == 0 {
                    match branch.left.clone() {
                        Some(node_hash) => {
                            current_node = db.get(&node_hash).unwrap().clone();
                            current_node_pos = 0;
                        }
                        None => {
                            branch.left = Some(new_leaf.hash.clone().unwrap());
                            new_leaf.store(db);
                            // don't do this here, do it when re-hashing the trie.
                            //branch.hash_and_store(db);
                            modified_nodes.push((0, Node::Branch(branch.clone())));
                            break;
                        }
                    }
                } else {
                    match branch.right.clone() {
                        Some(node_hash) => {
                            current_node = db.get(&node_hash).unwrap().clone();
                            current_node_pos = 1;
                        }
                        None => {
                            branch.left = Some(new_leaf.hash.clone().unwrap());
                            new_leaf.store(db);
                            // don't do this here, do it when re-hashing the Trie.
                            //branch.hash_and_store(db);
                            modified_nodes.push((1, Node::Branch(branch.clone())));
                            break;
                        }
                    }
                }
            }
            Node::Leaf(leaf) => {
                if update == false {
                    let neq_idx = find_key_idx_not_eq(&new_leaf.key[idx..].to_vec(), &leaf.key)
                        .expect("Can't insert duplicate Leaf");
                    let new_leaf_pos: u8 = new_leaf.key[neq_idx];
                    // there might be an inefficiency to this?
                    // we store leaf again with just a different prefix
                    // maybe won't do this in a future release...
                    leaf.prefix = Some(leaf.key[neq_idx..].to_vec());
                    new_leaf.prefix = Some(new_leaf.key[neq_idx..].to_vec());
                    // replace this leaf with a branch in memory
                    // re-hashing leafs here because of prefix change
                    leaf.hash_and_store(db);
                    new_leaf.hash_and_store(db);
                    let mut new_branch: Branch = Branch::empty(new_leaf.key[..neq_idx].to_vec());
                    if new_leaf_pos == 0 {
                        new_branch.left = new_leaf.hash.clone();
                        new_branch.right = leaf.hash.clone();
                    } else {
                        new_branch.left = leaf.hash.clone();
                        new_branch.right = new_leaf.hash.clone();
                    }
                    modified_nodes.push((current_node_pos, Node::Branch(new_branch)));
                    break;
                } else {
                    if let Some(_) = find_key_idx_not_eq(&new_leaf.key[idx..].to_vec(), &leaf.key) {
                        panic!("Can't update Leaf since it does not exist");
                    }
                    new_leaf.prefix = leaf.prefix.clone();
                    new_leaf.hash_and_store(db);
                    let new_branch = modified_nodes
                        .get(modified_nodes.len() - 1)
                        .expect("Leaf must have Branch or Root above it")
                        .clone();
                    match new_branch.1 {
                        Node::Root(mut root) => {
                            if current_node_pos == 0 {
                                root.left = Some(new_leaf.hash.clone().unwrap());
                            } else {
                                root.right = Some(new_leaf.hash.clone().unwrap());
                            }
                        }
                        Node::Branch(mut branch) => {
                            if current_node_pos == 0 {
                                branch.left = Some(new_leaf.hash.clone().unwrap());
                            } else {
                                branch.right = Some(new_leaf.hash.clone().unwrap());
                            }
                        }
                        _ => panic!("Parent must be Branch or Root"),
                    };
                    break;
                }
            }
        }
    }
    (modified_nodes, new_root)
}

fn update_modified_leafs(
    db: &mut InMemoryDB,
    mut modified_nodes: Vec<(u8, Node)>,
    mut new_root: Root,
) -> Root {
    modified_nodes.reverse();
    for chunk in &mut modified_nodes.chunks(2) {
        if let [child, parent] = chunk {
            // todo: re-hash child and insert it
            // todo: hash child, insert it's hash into the parent and re-hash the parent
            // insert both child and parent into the DB
            let child_node = child.1.clone();
            let child_idx = child.0;
            let parent_node = parent.1.clone();
            match parent_node {
                Node::Root(mut root) => match child_node {
                    Node::Leaf(mut leaf) => {
                        leaf.hash();
                        if child_idx == 0 {
                            root.left = Some(leaf.hash.clone().unwrap());
                        } else {
                            root.right = Some(leaf.hash.clone().unwrap());
                        }
                        leaf.store(db);
                        new_root = root.clone();
                    }
                    Node::Branch(mut branch) => {
                        branch.hash();
                        if child_idx == 0 {
                            root.left = Some(branch.hash.clone().unwrap());
                        } else {
                            root.right = Some(branch.hash.clone().unwrap());
                        }
                        branch.store(db);
                        new_root = root.clone();
                    }
                    _ => panic!("Child can't be a Root"),
                },
                Node::Branch(mut branch) => match child_node {
                    Node::Leaf(mut leaf) => {
                        leaf.hash();
                        if child_idx == 0 {
                            branch.left = Some(leaf.hash.clone().unwrap());
                        } else {
                            branch.right = Some(leaf.hash.clone().unwrap());
                        }
                        leaf.store(db);
                        branch.hash_and_store(db);
                    }
                    Node::Branch(mut branch) => {
                        branch.hash();
                        if child_idx == 0 {
                            branch.left = Some(branch.hash.clone().unwrap());
                        } else {
                            branch.right = Some(branch.hash.clone().unwrap());
                        }
                        branch.store(db);
                        branch.hash_and_store(db);
                    }
                    _ => panic!("Child can't be a Root"),
                },
                _ => panic!("Root can't be a child"),
            }
        }
    }
    new_root
}

fn find_key_idx_not_eq(k1: &Key, k2: &Key) -> Option<usize> {
    // todo: find the index at which the keys are not equal
    for (idx, digit) in k1.into_iter().enumerate() {
        if digit != &k2[idx] {
            return Some(idx);
        }
    }
    return None;
}

#[cfg(test)]
mod tests {
    use crate::merkle::tests::{generate_random_data, generate_random_key};
    use crate::store::types::{Hashable, Node, Root};
    use crate::store::{db::InMemoryDB, types::Leaf};
    use crate::{insert_leaf, update_leaf};
    use colored::*;
    use indicatif::ProgressBar;
    use std::collections::HashMap;
    use std::time::Instant;
    #[test]
    fn test_insert_leaf() {
        let start_time = Instant::now();
        let mut db = InMemoryDB {
            nodes: HashMap::new(),
        };
        let mut leaf_1: Leaf = Leaf::empty(vec![0u8; 256]);
        let mut leaf_2_key: Vec<u8> = vec![0; 253];
        for _i in 0..3 {
            leaf_2_key.push(1);
        }
        let mut leaf_2: Leaf = Leaf::empty(leaf_2_key);

        let mut leaf_3_key: Vec<u8> = vec![0; 253];
        for _i in 0..3 {
            leaf_3_key.push(0);
        }
        let mut leaf_3 = Leaf::empty(leaf_3_key);
        leaf_1.hash();
        leaf_2.hash();
        leaf_3.hash();
        let root: Root = Root::empty();
        let root_node = Node::Root(root);
        let new_root = insert_leaf(&mut db, &mut leaf_1, root_node);
        let new_root = insert_leaf(&mut db, &mut leaf_2, Node::Root(new_root));
        assert_eq!(
            new_root.hash.unwrap(),
            Root {
                hash: Some(vec![
                    170, 229, 131, 77, 235, 12, 173, 127, 222, 26, 105, 40, 22, 13, 179, 45, 178,
                    246, 170, 244, 16, 171, 204, 67, 102, 94, 208, 139, 143, 112, 136, 169
                ]),
                left: Some(vec![
                    192, 255, 218, 137, 120, 169, 46, 169, 51, 142, 15, 1, 84, 251, 124, 134, 95,
                    25, 100, 240, 136, 56, 116, 145, 21, 237, 3, 48, 55, 36, 46, 197
                ]),
                right: None
            }
            .hash
            .unwrap()
        );
        println!(
            "{} Elapsed Time: {} µs",
            "[1x Insert]".yellow(),
            &start_time.elapsed().as_micros().to_string().blue()
        );
    }
    #[test]
    fn test_update_leaf() {
        let start_time = Instant::now();
        let mut db = InMemoryDB {
            nodes: HashMap::new(),
        };
        let mut leaf_1: Leaf = Leaf::empty(vec![0u8; 256]);
        leaf_1.hash();
        let root: Root = Root::empty();
        let root_node = Node::Root(root);
        let new_root = insert_leaf(&mut db, &mut leaf_1, root_node);
        let mut leaf_1_updated: Leaf = Leaf::empty(vec![0; 256]);
        leaf_1_updated.data = Some(vec![1]);
        let _new_root = update_leaf(&mut db, &mut leaf_1_updated, Node::Root(new_root));
        let _leaf_from_db = db.get(&leaf_1_updated.hash.unwrap()).unwrap();
        println!(
            "{} Elapsed Time: {} µs",
            "[1x Update]".yellow(),
            &start_time.elapsed().as_micros().to_string().blue()
        );
    }
    #[test]
    fn test_insert_leafs() {
        let transaction_count: u32 = std::env::var("INSERT_TRANSACTION_COUNT")
            .unwrap_or_else(|_| "100000".to_string())
            .parse::<u32>()
            .expect("Invalid argument STRESS_TEST_TRANSACTION_COUNT");
        let mut transactions: Vec<Leaf> = Vec::new();
        for _ in 0..transaction_count {
            let leaf_key = generate_random_key();
            let leaf: Leaf = Leaf::new(leaf_key, Some(generate_random_data()));
            transactions.push(leaf);
        }
        let start_time = Instant::now();
        let mut db = InMemoryDB {
            nodes: HashMap::new(),
        };
        let root: Root = Root::empty();
        let mut root_node = Node::Root(root);
        let progress_bar: ProgressBar = ProgressBar::new(transaction_count as u64);
        for mut leaf in transactions {
            leaf.hash();
            let new_root = insert_leaf(&mut db, &mut leaf, root_node);
            root_node = Node::Root(new_root.clone());
            progress_bar.inc(1);
        }
        progress_bar.finish_with_message("Done testing insert!");
        println!(
            "[{}x Insert] Elapsed Time: {} s",
            transaction_count.to_string().yellow(),
            &start_time.elapsed().as_secs().to_string().blue()
        );
        println!("Memory DB size: {}", &db.nodes.len().to_string().blue());
    }
}
