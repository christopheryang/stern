use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub alg: String,
    pub m: u32,
    pub t: u32,
    pub p: u32,
    pub version: u32,
}

impl KdfParams {
    #[must_use]
    pub fn argon2id_default() -> Self {
        Self {
            alg: "argon2id".to_owned(),
            m: 262_144,
            t: 3,
            p: 4,
            version: 1,
        }
    }

    #[cfg(debug_assertions)]
    #[must_use]
    pub fn fast() -> Self {
        Self {
            alg: "argon2id".to_owned(),
            m: 8,
            t: 1,
            p: 1,
            version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2id_default_values() {
        let p = KdfParams::argon2id_default();
        assert_eq!(p.alg, "argon2id");
        assert_eq!(p.m, 262_144);
        assert_eq!(p.t, 3);
        assert_eq!(p.p, 4);
        assert_eq!(p.version, 1);
    }

    #[test]
    fn fast_values_are_small() {
        let p = KdfParams::fast();
        assert_eq!(p.alg, "argon2id");
        assert_eq!(p.m, 8);
        assert_eq!(p.t, 1);
        assert_eq!(p.p, 1);
        assert_eq!(p.version, 1);
    }

    #[test]
    fn serde_roundtrip() {
        let params = KdfParams::argon2id_default();
        let json = serde_json::to_string(&params).expect("serialize KdfParams");
        let back: KdfParams = serde_json::from_str(&json).expect("deserialize KdfParams");
        assert_eq!(params, back);
    }

    #[test]
    fn serde_roundtrip_fast() {
        let params = KdfParams::fast();
        let json = serde_json::to_string(&params).expect("serialize");
        let back: KdfParams = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(params, back);
    }

    #[test]
    fn clone_eq() {
        let p1 = KdfParams::argon2id_default();
        let p2 = p1.clone();
        assert_eq!(p1, p2);
    }
}
