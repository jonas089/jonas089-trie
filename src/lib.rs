use store::{
    db::Database,
    types::{Branch, Key, Leaf, Node, Root},
};

pub mod merkle;
pub mod store;

pub fn check_leaf(db: &mut dyn Database, leaf_expected: Leaf, mut current_node: Node) -> bool {
    // start with the root, if the child at the idx is a branch, check the prefix.
    // if the child at the idx is a leaf, compare keys
    let mut idx = 0;
    let mut result: bool = false;
    while idx < leaf_expected.key.len() {
        let digit: u8 = leaf_expected.key[idx];
        match &current_node {
            Node::Branch(branch) => {
                let branch_prefix = &branch.key;
                //if branch_prefix.as_slice() == &leaf_expected.key[0..branch_prefix.len()] {
                let neq_idx = &branch_prefix[0];
                let child_idx = leaf_expected.key[*neq_idx as usize];
                idx += branch_prefix.len();
                println!("Branch key: {:?}", &branch.key);
                if child_idx == 0 {
                    current_node = db.get(&branch.left.clone().unwrap()).unwrap().clone();
                } else {
                    current_node = db.get(&branch.right.clone().unwrap()).unwrap().clone();
                }
                /*} else {
                    panic!(
                        "Branch prefix mismatch: {:?} ||||||| {:?}",
                        &leaf_expected.key[0..branch_prefix.len()],
                        &branch_prefix.as_slice(),
                    )
                }*/
            }
            Node::Leaf(leaf) => {
                println!("Found leaf: {:?}", &leaf);
                assert_eq!(leaf.key, leaf_expected.key);
                result = true;
                break;
            }
            Node::Root(root) => {
                if digit == 0 {
                    match &root.left {
                        Some(node) => {
                            current_node = db.get(&node).unwrap().clone();
                            idx += 1;
                        }
                        None => panic!("Root does not have path"),
                    }
                } else {
                    match &root.right {
                        Some(node) => {
                            current_node = db.get(&node).unwrap().clone();
                            idx += 1;
                        }
                        None => panic!("Root does not have path"),
                    }
                }
            }
        }
    }
    result
}

pub fn insert_leaf(db: &mut dyn Database, new_leaf: &mut Leaf, root_node: Node) -> Root {
    assert_eq!(new_leaf.key.len(), 256);
    let (modified_nodes, new_root) = traverse_trie(db, new_leaf, root_node, false);
    let mut new_root = update_modified_leafs(db, modified_nodes, new_root);
    new_root.hash_and_store(db);
    new_root
}

pub fn update_leaf(db: &mut dyn Database, new_leaf: &mut Leaf, root_node: Node) -> Root {
    let (modified_nodes, new_root) = traverse_trie(db, new_leaf, root_node, true);
    let mut new_root = update_modified_leafs(db, modified_nodes, new_root);
    new_root.hash_and_store(db);
    new_root
}

fn traverse_trie(
    db: &mut dyn Database,
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
        let digit: u8 = new_leaf.key[idx];
        assert!(digit == 0 || digit == 1);
        match &mut current_node {
            Node::Root(root) => {
                if digit == 0 {
                    match root.left.clone() {
                        Some(node_hash) => {
                            let root_unwrapped: Root = root_node.clone().unwrap_as_root();
                            modified_nodes.push((0, Node::Root(root_unwrapped)));
                            current_node = db.get(&node_hash).unwrap().clone();
                            current_node_pos = 0;
                        }
                        None => {
                            let mut root: Root = current_node.unwrap_as_root();
                            root.left = Some(new_leaf.hash.clone().unwrap());
                            new_root = root.clone();
                            new_leaf.store(db);
                            modified_nodes.push((0, Node::Root(root)));
                            modified_nodes.push((0, Node::Leaf(new_leaf.clone())));
                            break;
                        }
                    }
                } else {
                    match root.right.clone() {
                        Some(node_hash) => {
                            let root_unwrapped: Root = root_node.clone().unwrap_as_root();
                            modified_nodes.push((0, Node::Root(root_unwrapped)));
                            current_node = db.get(&node_hash).unwrap().clone();
                            current_node_pos = 1;
                        }
                        None => {
                            let mut root = current_node.clone().unwrap_as_root();
                            root.right = Some(new_leaf.hash.clone().unwrap());
                            new_root = root.clone();
                            new_leaf.store(db);
                            modified_nodes.push((0, Node::Root(root)));
                            modified_nodes.push((1, Node::Leaf(new_leaf.clone())));
                            break;
                        }
                    }
                }
            }
            Node::Branch(branch) => {
                idx = branch.key[0] as usize;
                if new_leaf.key[idx] == 0 {
                    match branch.left.clone() {
                        Some(node_hash) => {
                            modified_nodes.push((current_node_pos, Node::Branch(branch.clone())));
                            current_node = db.get(&node_hash).unwrap().clone();
                            current_node_pos = 0;
                        }
                        None => {
                            panic!("A branch must have 2 children")
                        }
                    }
                } else {
                    match branch.right.clone() {
                        Some(node_hash) => {
                            modified_nodes.push((current_node_pos, Node::Branch(branch.clone())));
                            current_node = db.get(&node_hash).unwrap().clone();
                            current_node_pos = 1;
                        }
                        None => {
                            panic!("A branch must have 2 children")
                        }
                    }
                }
            }
            Node::Leaf(leaf) => {
                if !update {
                    let neq_idx = find_key_idx_not_eq(&new_leaf.key, &leaf.key)
                        .expect("Can't insert duplicate Leaf");
                    let new_leaf_pos: u8 = new_leaf.key[neq_idx];
                    // there might be an inefficiency to this?
                    // we store leaf again with just a different prefix
                    // maybe don't do this in a future release...
                    //leaf.prefix = Some(leaf.key[neq_idx..].to_vec());
                    //new_leaf.prefix = Some(new_leaf.key[neq_idx..].to_vec());
                    leaf.hash_and_store(db);
                    new_leaf.hash_and_store(db);
                    let mut new_branch: Branch = Branch::empty(vec![neq_idx as u8]);
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
                    todo!("Currently not supported");
                }
            }
        }
    }
    (modified_nodes, new_root)
}

fn update_modified_leafs(
    db: &mut dyn Database,
    mut modified_nodes: Vec<(u8, Node)>,
    mut new_root: Root,
) -> Root {
    modified_nodes.reverse();
    assert!(modified_nodes.len() >= 2);
    let modified_clone = modified_nodes.clone();
    for (cnt, chunk) in modified_clone.chunks(2).enumerate() {
        if let [_, _] = chunk {
            let idx = cnt * 2 + 1;
            // todo: re-hash child and insert it
            // todo: hash child, insert it's hash into the parent and re-hash the parent
            // insert both child and parent into the DB
            let child_node: Node = modified_nodes[idx - 1].clone().1;
            let parent_node = modified_nodes[idx].clone().1;
            let child_idx = modified_nodes[idx - 1].clone().0;
            let maybe_new_branch: Option<Branch> = match parent_node {
                Node::Root(mut root) => match child_node {
                    Node::Leaf(leaf) => {
                        if child_idx == 0 {
                            root.left = Some(leaf.hash.clone().unwrap());
                        } else {
                            root.right = Some(leaf.hash.clone().unwrap());
                        }
                        new_root = root.clone();
                        None
                    }
                    Node::Branch(mut branch) => {
                        match branch.hash {
                            Some(_) => {}
                            None => branch.hash_and_store(db),
                        }
                        if child_idx == 0 {
                            root.left = Some(branch.hash.clone().unwrap());
                        } else {
                            root.right = Some(branch.hash.clone().unwrap());
                        }
                        new_root = root.clone();
                        None
                    }
                    _ => panic!("This should never happen"),
                },
                Node::Branch(mut parent_branch) => match child_node {
                    Node::Branch(mut branch) => {
                        branch.hash_and_store(db);
                        if child_idx == 0 {
                            parent_branch.left = Some(branch.hash.clone().unwrap());
                        } else {
                            parent_branch.right = Some(branch.hash.clone().unwrap());
                        }
                        parent_branch.hash_and_store(db);
                        //panic!("Parent branch involved");
                        Some(parent_branch)
                    }
                    _ => None,
                },
                _ => {
                    panic!("This should never happen")
                }
            };
            match maybe_new_branch {
                Some(branch) => {
                    modified_nodes.insert(idx, (modified_nodes[idx].0, Node::Branch(branch)))
                }
                None => {}
            }
        } else {
            let mut root = modified_nodes[modified_nodes.len() - 1]
                .1
                .clone()
                .unwrap_as_root();
            let branch = modified_nodes[modified_nodes.len() - 2].clone();
            let branch_unwrapped = branch.1.unwrap_as_branch();
            if branch.0 == 0 {
                root.left = Some(branch_unwrapped.hash.unwrap());
            } else {
                root.right = Some(branch_unwrapped.hash.unwrap());
            }
            new_root = root;
        }
    }
    new_root
}

fn find_key_idx_not_eq(k1: &Key, k2: &Key) -> Option<usize> {
    // todo: find the index at which the keys are not equal
    for (idx, digit) in k1.iter().enumerate() {
        if digit != &k2[idx] {
            return Some(idx);
        }
    }
    None
}

#[test]
fn test_find_key_neq() {
    let x = vec![0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1];
    let y = vec![0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1];
    assert_eq!(find_key_idx_not_eq(&x, &y).unwrap(), 9);
}

#[cfg(test)]
mod tests {
    use crate::merkle::tests::{generate_random_data, generate_random_key};
    #[cfg(feature = "sqlite")]
    use crate::store::db::sql::TrieDB;
    #[cfg(not(feature = "sqlite"))]
    use crate::store::db::TrieDB;
    use crate::store::types::Leaf;
    use crate::store::types::{Hashable, Node, Root};
    use crate::{check_leaf, insert_leaf};
    use colored::*;
    use indicatif::ProgressBar;
    use std::collections::HashMap;
    use std::time::Instant;
    #[test]
    fn test_insert_leaf() {
        let start_time = Instant::now();
        #[cfg(not(feature = "sqlite"))]
        let mut db = TrieDB {
            nodes: HashMap::new(),
        };

        #[cfg(feature = "sqlite")]
        let mut db = TrieDB {
            path: env::var("PATH_TO_DB").unwrap_or("database.sqlite".to_string()),
            cache: None,
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
        let new_root = insert_leaf(&mut db, &mut leaf_1, root_node.clone());
        let _ = insert_leaf(&mut db, &mut leaf_2, Node::Root(new_root));

        println!(
            "{} Elapsed Time: {} µs",
            "[1x Insert]".yellow(),
            &start_time.elapsed().as_micros().to_string().blue()
        );
    }

    #[test]
    fn test_many_leafs() {
        let transaction_count: u32 = std::env::var("INSERT_TRANSACTION_COUNT")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u32>()
            .expect("Invalid argument STRESS_TEST_TRANSACTION_COUNT");
        let mut transactions: Vec<Leaf> = Vec::new();
        for _ in 0..transaction_count {
            let leaf_key = generate_random_key();
            let leaf: Leaf = Leaf::new(leaf_key, Some(generate_random_data()));
            transactions.push(leaf);
        }
        let start_time = Instant::now();
        #[cfg(not(feature = "sqlite"))]
        let mut db = TrieDB {
            nodes: HashMap::new(),
        };

        #[cfg(feature = "sqlite")]
        let mut db = TrieDB {
            path: env::var("PATH_TO_DB").unwrap_or("database.sqlite".to_string()),
            cache: None,
        };
        let root: Root = Root::empty();
        let mut root_node = Node::Root(root);
        let progress_bar: ProgressBar = ProgressBar::new(transaction_count as u64);
        for mut leaf in transactions {
            leaf.hash();
            let new_root = insert_leaf(&mut db, &mut leaf, root_node.clone());
            assert!(check_leaf(
                &mut db,
                leaf.clone(),
                Node::Root(new_root.clone())
            ));
            root_node = Node::Root(new_root.clone());
            progress_bar.inc(1);
        }
        progress_bar.finish_with_message("Done testing insert!");
        println!(
            "[{}x Insert] Elapsed Time: {} s",
            transaction_count.to_string().yellow(),
            &start_time.elapsed().as_secs().to_string().blue()
        );
        #[cfg(not(feature = "sqlite"))]
        println!("Memory DB size: {}", &db.nodes.len().to_string().blue());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn test_sql_db() {
        use crate::merkle::{merkle_proof, verify_merkle_proof};
        use crate::store::db::sql::TrieDB;

        let start_time = Instant::now();
        let mut db = TrieDB {
            path: env::var("PATH_TO_DB").unwrap_or("database.sqlite".to_string()),
            cache: None,
        };
        db.setup();
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
            new_root.hash.clone().unwrap(),
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
        let mut leaf_1: Leaf = Leaf::empty(vec![1u8; 256]);
        leaf_1.hash();
        let root: Root = new_root;
        let root_node: Node = Node::Root(root);
        let new_root: Root = insert_leaf(&mut db, &mut leaf_1, root_node);
        let proof = merkle_proof(&mut db, leaf_1.key, Node::Root(new_root.clone()));
        let inner_proof = proof.unwrap().nodes;
        verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap());
    }
}
