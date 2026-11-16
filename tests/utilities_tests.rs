use std::num::NonZeroU64;

use elykseer_lib::common::utilities::{
    drop_list, file_hash, is_hex_id, make_list, rnd, rnd256, sha3_256, take_list,
};

// ===========================================================================
// make_list
// ===========================================================================

#[cfg(test)]
mod make_list_tests {
    use super::*;

    #[test]
    fn make_list_one() {
        let result = make_list(NonZeroU64::new(1).unwrap());
        assert_eq!(result, vec![NonZeroU64::new(1).unwrap()]);
    }

    #[test]
    fn make_list_five() {
        let result = make_list(NonZeroU64::new(5).unwrap());
        let expected: Vec<NonZeroU64> = (1..=5).map(|x| NonZeroU64::new(x).unwrap()).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn make_list_length() {
        let n = 16u64;
        let result = make_list(NonZeroU64::new(n).unwrap());
        assert_eq!(result.len(), n as usize);
    }

    #[test]
    fn make_list_first_is_one() {
        let result = make_list(NonZeroU64::new(10).unwrap());
        assert_eq!(result[0], NonZeroU64::new(1).unwrap());
    }

    #[test]
    fn make_list_last_is_n() {
        let n = 42u64;
        let result = make_list(NonZeroU64::new(n).unwrap());
        assert_eq!(*result.last().unwrap(), NonZeroU64::new(n).unwrap());
    }
}

// ===========================================================================
// drop_list
// ===========================================================================

#[cfg(test)]
mod drop_list_tests {
    use super::*;

    #[test]
    fn drop_zero_is_identity() {
        let ls = vec![0, 1, 2];
        assert_eq!(drop_list(0, &ls), ls);
    }

    #[test]
    fn drop_one() {
        let ls = vec![0, 1, 2];
        assert_eq!(drop_list(1, &ls), vec![1, 2]);
    }

    #[test]
    fn drop_all() {
        let ls = vec![0, 1, 2];
        assert_eq!(drop_list(3, &ls), Vec::<i32>::new());
    }

    #[test]
    fn drop_more_than_length() {
        let ls = vec![0, 1, 2];
        assert_eq!(drop_list(10, &ls), Vec::<i32>::new());
    }

    #[test]
    fn drop_from_empty() {
        let ls: Vec<i32> = vec![];
        assert_eq!(drop_list(5, &ls), Vec::<i32>::new());
    }
}

// ===========================================================================
// take_list
// ===========================================================================

#[cfg(test)]
mod take_list_tests {
    use super::*;

    #[test]
    fn take_zero_is_empty() {
        let ls = vec![0, 1, 2, 3];
        assert_eq!(take_list(0, &ls), Vec::<i32>::new());
    }

    #[test]
    fn take_two() {
        let ls = vec![0, 1, 2, 3];
        assert_eq!(take_list(2, &ls), vec![0, 1]);
    }

    #[test]
    fn take_all() {
        let ls = vec![0, 1, 2, 3];
        assert_eq!(take_list(4, &ls), ls);
    }

    #[test]
    fn take_more_than_length() {
        let ls = vec![0, 1, 2];
        assert_eq!(take_list(10, &ls), ls);
    }

    #[test]
    fn take_from_empty() {
        let ls: Vec<i32> = vec![];
        assert_eq!(take_list(3, &ls), Vec::<i32>::new());
    }
}

// ===========================================================================
// rnd
// ===========================================================================

#[cfg(test)]
mod rnd_tests {
    use super::*;

    #[test]
    fn rnd_returns_nonzero() {
        let r = rnd(0);
        // Very unlikely to be zero
        assert_ne!(r, 0);
    }

    #[test]
    fn rnd_ignores_argument() {
        // rnd ignores its argument per Coq spec; successive calls differ due to time
        let r1 = rnd(42);
        let r2 = rnd(99);
        // Both should produce some value (can't assert equality or inequality reliably)
        assert!(r1 > 0 || r2 > 0);
    }
}

// ===========================================================================
// rnd256
// ===========================================================================

#[cfg(test)]
mod rnd256_tests {
    use super::*;

    #[test]
    fn rnd256_returns_64_hex_chars() {
        let r = rnd256("test");
        assert_eq!(r.len(), 64);
        assert!(r.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rnd256_different_seeds_differ() {
        let r1 = rnd256("seed_a");
        let r2 = rnd256("seed_b");
        assert_ne!(r1, r2);
    }

    #[test]
    fn rnd256_empty_seed() {
        let r = rnd256("");
        assert_eq!(r.len(), 64);
        assert!(r.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rnd256_same_seed_differs() {
        let ids: std::collections::HashSet<String> = (0..1000).map(|_| rnd256("same")).collect();
        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn rnd256_is_hex_id() {
        assert!(is_hex_id(&rnd256("me")));
    }
}

// ===========================================================================
// sha3_256
// ===========================================================================

#[cfg(test)]
mod sha3_256_tests {
    use super::*;

    #[test]
    fn sha3_256_returns_64_hex_chars() {
        let h = sha3_256("hello");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha3_256_deterministic() {
        let h1 = sha3_256("hello");
        let h2 = sha3_256("hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn sha3_256_different_inputs_differ() {
        let h1 = sha3_256("hello");
        let h2 = sha3_256("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn sha3_256_empty_input() {
        let h = sha3_256("");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha3_256_known_answers() {
        assert_eq!(
            sha3_256(""),
            "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
        );
        assert_eq!(
            sha3_256("abc"),
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
    }
}

// ===========================================================================
// file_hash, is_hex_id
// ===========================================================================

#[cfg(test)]
mod file_hash_tests {
    use super::*;

    #[test]
    fn file_hash_is_sha3_of_fname_and_my_id() {
        assert_eq!(file_hash("/tmp/file", "me"), sha3_256("/tmp/fileme"));
        assert_eq!(
            file_hash("/tmp/file", "me"),
            "2c0f2f6010001143ae3abd947a75c8307c19ba0b94a6602fd6bbfc2f13b7f0d2"
        );
    }

    #[test]
    fn file_hash_depends_on_my_id() {
        assert_ne!(file_hash("/tmp/file", "a"), file_hash("/tmp/file", "b"));
    }

    #[test]
    fn is_hex_id_accepts_64_lowercase_hex() {
        assert!(is_hex_id(&"0a".repeat(32)));
    }

    #[test]
    fn is_hex_id_rejects_others() {
        assert!(!is_hex_id(""));
        assert!(!is_hex_id(&"a".repeat(63)));
        assert!(!is_hex_id(&"a".repeat(65)));
        assert!(!is_hex_id(&"A".repeat(64)));
        assert!(!is_hex_id(&format!("../{}", "a".repeat(61))));
        assert!(!is_hex_id(&format!("{}/x", "a".repeat(62))));
    }
}
