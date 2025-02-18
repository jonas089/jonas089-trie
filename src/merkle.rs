use serde::{Deserialize, Serialize};

// Compute Merkle Proof for a Leaf at a given point in time (e.g. at a Snapshot)
use crate::store::{
    db::Database,
    types::{Hashable, Node, NodeHash, RootHash},
};
use anyhow::{bail, Result};
// obtain the merkle path for a leaf
pub fn merkle_proof(db: &mut dyn Database, key: Vec<u8>, trie_root: Node) -> Result<MerkleProof> {
    assert_eq!(key.len(), 256);
    let mut proof: MerkleProof = MerkleProof { nodes: Vec::new() };
    let mut current_node = trie_root.clone();
    loop {
        match &mut current_node {
            Node::Root(root) => {
                proof.nodes.push((false, Node::Root(root.clone())));
                if key[0] == 0 {
                    let left_child = db.get(&root.left.clone().unwrap()).unwrap();
                    current_node = left_child.clone();
                    proof.nodes.push((false, left_child.clone()));
                } else {
                    let right_child = db.get(&root.right.clone().unwrap()).unwrap();
                    current_node = right_child.clone();
                    proof.nodes.push((true, right_child.clone()));
                }
            }
            Node::Branch(branch) => {
                let digit = key[branch.key[0] as usize];
                if digit == 0 {
                    current_node = db.get(&branch.left.clone().unwrap()).unwrap().clone();
                    proof.nodes.push((false, current_node.clone()));
                } else {
                    current_node = db.get(&branch.right.clone().unwrap()).unwrap().clone();
                    proof.nodes.push((true, current_node.clone()));
                }
            }
            Node::Leaf(_) => return Ok(proof),
        }
    }
}

pub fn verify_merkle_proof(
    mut inner_proof: Vec<(bool, Node)>,
    state_root_hash: RootHash,
) -> Result<()> {
    inner_proof.reverse();
    let mut current_hash: Option<(bool, NodeHash)> = None;
    let mut root_hash: Option<RootHash> = None;
    for (idx, node) in inner_proof.into_iter().enumerate() {
        if idx == 0 {
            let leaf = node.1.unwrap_as_leaf()?;
            current_hash = Some((node.0, leaf.hash.unwrap()));
        } else {
            match node.1 {
                Node::Root(mut root) => {
                    if !current_hash.clone().unwrap().0 {
                        root.left = Some(current_hash.clone().unwrap().1);
                    } else {
                        root.right = Some(current_hash.clone().unwrap().1);
                    }
                    root.hash();
                    root_hash = root.hash;
                }
                Node::Branch(mut branch) => {
                    if !current_hash.clone().unwrap().0 {
                        branch.left = Some(current_hash.clone().unwrap().1);
                    } else {
                        branch.right = Some(current_hash.clone().unwrap().1);
                    }
                    branch.hash();
                    current_hash = Some((node.0, branch.hash.unwrap()));
                }
                Node::Leaf(_) => bail!("Invalid Node variant in Merkle Proof"),
            }
        }
    }
    // if this assertion passes, the merkle proof is valid
    // for the given root hash
    assert_eq!(&state_root_hash, &root_hash.unwrap());
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    pub nodes: Vec<(bool, Node)>,
}

#[cfg(test)]
pub mod tests {
    use crate::store::db::sql::TrieDB;
    use crate::{
        insert_leaf,
        merkle::verify_merkle_proof,
        store::types::{Hashable, Key, Leaf, Node, NodeHash, Root},
    };
    use std::{env, time::Instant};

    use super::merkle_proof;
    use colored::*;

    #[test]
    fn test_merkle_proof() {
        let mut db = TrieDB {
            path: env::var("PATH_TO_DB").unwrap_or("database.sqlite".to_string()),
            cache: None,
        };
        db.setup();
        let mut leaf_1: Leaf = Leaf::empty(vec![0u8; 256]);
        leaf_1.hash();

        let mut leaf_2_key = vec![0, 0];
        for _i in 0..254 {
            leaf_2_key.push(1);
        }
        let mut leaf_2: Leaf = Leaf::empty(leaf_2_key);
        leaf_2.hash();
        let root: Root = Root::empty();
        let root_node: Node = Node::Root(root);
        let new_root: Root = insert_leaf(&mut db, &mut leaf_1, root_node).unwrap();
        let new_root: Root = insert_leaf(&mut db, &mut leaf_2, Node::Root(new_root)).unwrap();
        let proof = merkle_proof(&mut db, leaf_2.key, Node::Root(new_root.clone()));
        // verify merkle proof
        let inner_proof = proof.unwrap().nodes;
        assert_eq!(
            inner_proof
                .last()
                .unwrap()
                .clone()
                .1
                .unwrap_as_leaf()
                .unwrap()
                .hash,
            leaf_2.hash
        );
        verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap()).unwrap();

        let proof = merkle_proof(&mut db, leaf_1.key, Node::Root(new_root.clone()));
        let inner_proof = proof.unwrap().nodes;
        verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap()).unwrap();
    }

    #[test]
    fn simulate_insert_flow() {
        let mut db = TrieDB {
            path: env::var("PATH_TO_DB").unwrap_or("database.sqlite".to_string()),
            cache: None,
        };
        db.setup();
        let root: Root = Root::empty();
        let root_node: Node = Node::Root(root);
        let mut current_root = root_node.clone();
        let Message_count: u32 = env::var("STRESS_TEST_MESSAGE_COUNT")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u32>()
            .expect("Invalid argument STRESS_TEST_MESSAGE_COUNT");
        let progress_bar: ProgressBar = ProgressBar::new(Message_count as u64);
        let mut leafs: Vec<Leaf> = Vec::new();
        for _ in 0..Message_count {
            let leaf_key: Key = generate_random_key();
            let mut leaf: Leaf = Leaf::empty(leaf_key.clone());
            leaf.data = Some(generate_random_data());
            leafs.push(leaf);
        }
        let mut leaf_keys: Vec<NodeHash> = Vec::new();
        let start_time = Instant::now();
        for mut leaf in leafs {
            leaf.hash();
            let new_root: Root =
                insert_leaf(&mut db, &mut leaf.clone(), current_root.clone()).unwrap();
            let proof = merkle_proof(&mut db, leaf.key.clone(), Node::Root(new_root.clone()));
            let inner_proof = proof.unwrap().nodes;
            verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap()).unwrap();

            #[cfg(feature = "stress-test")]
            for key in leaf_keys.clone() {
                let proof = merkle_proof(&mut db, key, Node::Root(new_root.clone()));
                let mut inner_proof = proof.unwrap().nodes;
                verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap());
            }
            #[cfg(not(feature = "stress-test"))]
            {
                let proof = merkle_proof(&mut db, leaf.key.clone(), Node::Root(new_root.clone()));
                let inner_proof = proof.unwrap().nodes;
                verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap()).unwrap();
            }
            leaf_keys.push(leaf.key.clone());
            current_root = Node::Root(new_root.clone());
            progress_bar.inc(1);
        }
        progress_bar.finish_with_message("Done checking merkle proofs!");
        println!(
            "[{}x Merkle Proof] Elapsed Time: {} s",
            Message_count.to_string().yellow(),
            &start_time.elapsed().as_secs().to_string().blue()
        );
    }

    use indicatif::ProgressBar;
    use rand::Rng;
    pub fn generate_random_key() -> Key {
        let mut rng = rand::thread_rng();
        (0..256)
            .map(|_| if rng.gen_bool(0.5) { 1 } else { 0 })
            .collect()
    }
    pub fn generate_random_data() -> Key {
        let mut rng = rand::thread_rng();
        (0..256).map(|_| rng.gen_range(0..255)).collect()
    }
}
