// Compute Merkle Proof for a Leaf at a given point in time (e.g. at a Snapshot)
use crate::store::{
    db::InMemoryDB,
    types::{Hashable, Node, NodeHash, RootHash},
};
// obtain the merkle path for a leaf
pub fn merkle_proof(db: &mut InMemoryDB, key: Vec<u8>, trie_root: Node) -> Option<MerkleProof> {
    assert_eq!(key.len(), 256);
    let mut idx: usize = 0;
    let mut proof: MerkleProof = MerkleProof { nodes: Vec::new() };
    let mut current_node = trie_root.clone();
    let mut digit: u8 = key[idx];
    loop {
        match &mut current_node {
            Node::Root(root) => {
                proof.nodes.push((false, Node::Root(root.clone())));
                if digit == 0 {
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
                idx += branch.key.len();
                digit = key[idx];
                if digit == 0 {
                    current_node = db.get(&branch.left.clone().unwrap()).unwrap().clone();
                    proof.nodes.push((false, current_node.clone()));
                } else {
                    current_node = db.get(&branch.right.clone().unwrap()).unwrap().clone();
                    proof.nodes.push((true, current_node.clone()));
                }
            }
            Node::Leaf(_) => return Some(proof),
        }
    }
}

pub fn verify_merkle_proof(inner_proof: Vec<(bool, Node)>, state_root_hash: RootHash) {
    let mut current_hash: Option<(bool, NodeHash)> = None;
    let mut root_hash: Option<RootHash> = None;
    for (idx, node) in inner_proof.into_iter().enumerate() {
        if idx == 0 {
            // must be a leaf
            let mut leaf = node.1.unwrap_as_leaf();
            leaf.hash = None;
            leaf.hash();
            current_hash = Some((node.0, leaf.hash.unwrap()));
        } else {
            match node.1 {
                Node::Root(mut root) => {
                    if current_hash.clone().unwrap().0 == false {
                        root.left = Some(current_hash.clone().unwrap().1);
                    } else {
                        root.right = Some(current_hash.clone().unwrap().1);
                    }
                    root.hash = None;
                    root.hash();
                    root_hash = root.hash;
                }
                Node::Branch(mut branch) => {
                    if current_hash.clone().unwrap().0 == false {
                        branch.left = Some(current_hash.clone().unwrap().1);
                    } else {
                        branch.right = Some(current_hash.clone().unwrap().1);
                    }
                    branch.hash = None;
                    branch.hash();
                    current_hash = Some((node.0, branch.hash.unwrap()));
                }
                Node::Leaf(_) => panic!("Invalid Node variant in Merkle Proof"),
            }
        }
    }
    // if this assertion passes, the merkle proof is valid
    // for the given root hash
    assert_eq!(&state_root_hash, &root_hash.unwrap());
}

#[derive(Clone, Debug)]
pub struct MerkleProof {
    pub nodes: Vec<(bool, Node)>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        insert_leaf,
        merkle::verify_merkle_proof,
        store::{
            db::InMemoryDB,
            types::{Hashable, Key, Leaf, Node, Root, RootHash},
        },
    };

    use super::merkle_proof;

    #[test]
    fn test_merkle_proof() {
        let mut db = InMemoryDB {
            nodes: HashMap::new(),
        };
        let mut leaf_1: Leaf = Leaf::empty(vec![0u8; 256]);
        leaf_1.hash();

        let mut leaf_2_key = vec![0, 0];
        for _i in 0..254 {
            leaf_2_key.push(1);
        }
        let mut leaf_2: Leaf = Leaf::empty(leaf_2_key);
        let root: Root = Root::empty();
        let root_node: Node = Node::Root(root);
        let new_root: Root = insert_leaf(&mut db, &mut leaf_1, root_node);
        let new_root: Root = insert_leaf(&mut db, &mut leaf_2, Node::Root(new_root));
        let proof = merkle_proof(&mut db, leaf_2.key, Node::Root(new_root.clone()));

        // verify merkle proof
        let mut inner_proof = proof.unwrap().nodes;
        inner_proof.reverse();
        verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap());

        let proof = merkle_proof(&mut db, leaf_1.key, Node::Root(new_root.clone()));
        let mut inner_proof = proof.unwrap().nodes;
        inner_proof.reverse();
        verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap());
    }
    #[test]
    fn simulate_insert_flow() {
        let mut db = InMemoryDB {
            nodes: HashMap::new(),
        };
        let mut leaf_data: Vec<u8> = vec![0];
        let root: Root = Root::empty();
        let root_node: Node = Node::Root(root);
        let mut current_root = root_node.clone();
        let mut idx = 0;
        loop {
            let leaf_key: Key = generate_random_key();
            leaf_data.push(0);
            let mut leaf: Leaf = Leaf::empty(leaf_key);
            leaf.data = Some(leaf_data.clone());
            leaf.hash();
            let new_root: Root = insert_leaf(&mut db, &mut leaf, current_root.clone());

            let proof = merkle_proof(&mut db, leaf.key, Node::Root(new_root.clone()));
            let mut inner_proof = proof.unwrap().nodes;
            inner_proof.reverse();
            verify_merkle_proof(inner_proof, new_root.hash.clone().unwrap());

            current_root = Node::Root(new_root.clone());
            idx += 1;
            if idx >= 1000 {
                break;
            }
        }
    }

    use rand::Rng;
    fn generate_random_key() -> Key {
        let mut rng = rand::thread_rng();
        (0..256)
            .map(|_| if rng.gen_bool(0.5) { 1 } else { 0 })
            .collect()
    }
}
