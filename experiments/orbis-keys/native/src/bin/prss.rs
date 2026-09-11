//! Classic subset-seed PRSS storage/derivation fixture; no proactive refresh claim.
use decaf377::Fr;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde_json::json;
use sha2::Sha512;
use std::time::Instant;

fn subsets(n: usize, k: usize) -> Vec<Vec<usize>> {
    fn go(n: usize, k: usize, next: usize, a: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if a.len() == k {
            out.push(a.clone());
            return;
        }
        for i in next..n {
            a.push(i);
            go(n, k, i + 1, a, out);
            a.pop();
        }
    }
    let mut result = Vec::new();
    go(n, k, 0, &mut Vec::new(), &mut result);
    result
}
fn derive(n: usize, groups: &[Vec<usize>], seeds: &[[u8; 32]], identity: &[u8]) -> Vec<Fr> {
    let mut shares = vec![Fr::from(0u64); n];
    for (group, seed) in groups.iter().zip(seeds) {
        let mut h = Hmac::<Sha512>::new_from_slice(seed).unwrap();
        h.update(b"prss-experiment-v1\0");
        h.update(identity);
        let value = Fr::from_le_bytes_mod_order(&h.finalize().into_bytes());
        for &i in group {
            let x = Fr::from(i as u64 + 1);
            let mut factor = Fr::from(1u64);
            for j in 0..n {
                if !group.contains(&j) {
                    let y = Fr::from(j as u64 + 1);
                    factor *= (x - y) * (-y).inverse().unwrap();
                }
            }
            shares[i] += value * factor;
        }
    }
    shares
}
fn recover(shares: &[Fr], indices: &[usize]) -> Fr {
    let mut result = Fr::from(0u64);
    for &i in indices {
        let x = Fr::from(i as u64 + 1);
        let mut coeff = Fr::from(1u64);
        for &j in indices {
            if i != j {
                let y = Fr::from(j as u64 + 1);
                coeff *= (-y) * (x - y).inverse().unwrap();
            }
        }
        result += shares[i] * coeff;
    }
    result
}
fn main() {
    for (n, t) in [(3, 2), (5, 3), (7, 4), (9, 5)] {
        let groups = subsets(n, n - t + 1);
        let seeds: Vec<[u8; 32]> = groups
            .iter()
            .map(|_| {
                let mut s = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut s);
                s
            })
            .collect();
        let start = Instant::now();
        for i in 0u64..32 {
            let shares = derive(n, &groups, &seeds, &i.to_le_bytes());
            std::hint::black_box(shares);
        }
        let micros = start.elapsed().as_micros();
        let alice = derive(n, &groups, &seeds, b"Alice/amount");
        let key = recover(&alice, &(0..t).collect::<Vec<_>>());
        assert_eq!(key, recover(&alice, &(n - t..n).collect::<Vec<_>>()));
        let bob = recover(
            &derive(n, &groups, &seeds, b"Bob/amount"),
            &(0..t).collect::<Vec<_>>(),
        );
        assert_ne!(key, bob);
        let mut rotated = seeds.clone();
        rotated[0][0] ^= 1;
        let changed = recover(
            &derive(n, &groups, &rotated, b"Alice/amount"),
            &(0..t).collect::<Vec<_>>(),
        );
        assert_ne!(
            key, changed,
            "naive seed rotation must not be mistaken for share refresh"
        );
        let per_node = groups.iter().filter(|s| s.contains(&0)).count();
        println!(
            "{}",
            json!({"case":"classic subset-seed PRSS synthetic fixture","nodes":n,"threshold":t,"seeds_per_node":per_node,"secret_bytes_per_node":per_node*32,"batch":32,"all_nodes_serial_microseconds":micros,"naive_seed_rotation_changes_historical_keys":true,"production_refresh_protocol_tested":false})
        );
    }
}
