#[cfg(test)]
mod tests {
    use elykseer_lib::nchunks::Nchunks;

    #[test]
    fn test_nchunks_new() {
        let nchunks = Nchunks::new(16);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::new(256);
        assert_eq!(nchunks.to_positive(), 256);

        let nchunks = Nchunks::new(0);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::new(300);
        assert_eq!(nchunks.to_positive(), 256);
    }

    #[test]
    fn test_nchunks_from_positive() {
        let nchunks = Nchunks::from_positive(16);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::from_positive(256);
        assert_eq!(nchunks.to_positive(), 256);

        let nchunks = Nchunks::from_positive(0);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::from_positive(300);
        assert_eq!(nchunks.to_positive(), 256);
    }

    #[test]
    fn test_nchunks_from_int() {
        let nchunks = Nchunks::from_int(16);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::from_int(256);
        assert_eq!(nchunks.to_positive(), 256);

        let nchunks = Nchunks::from_int(0);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::from_int(300);
        assert_eq!(nchunks.to_positive(), 256);

        let nchunks = Nchunks::from_int(-10);
        assert_eq!(nchunks.to_positive(), 16);
    }

    #[test]
    fn test_nchunks_to_positive() {
        let nchunks = Nchunks::new(16);
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::new(256);
        assert_eq!(nchunks.to_positive(), 256);
    }

    #[test]
    fn test_nchunks_to_n() {
        let nchunks = Nchunks::new(16);
        assert_eq!(nchunks.to_n(), 16);

        let nchunks = Nchunks::new(256);
        assert_eq!(nchunks.to_n(), 256);
    }

    #[test]
    fn test_nchunks_try_from_u32() {
        let nchunks = Nchunks::try_from(16).unwrap();
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::try_from(256).unwrap();
        assert_eq!(nchunks.to_positive(), 256);

        let nchunks = Nchunks::try_from(0);
        assert!(nchunks.is_err());

        let nchunks = Nchunks::try_from(300);
        assert!(nchunks.is_err());
    }

    #[test]
    fn test_nchunks_try_from_i32() {
        let nchunks = Nchunks::try_from(16).unwrap();
        assert_eq!(nchunks.to_positive(), 16);

        let nchunks = Nchunks::try_from(256).unwrap();
        assert_eq!(nchunks.to_positive(), 256);

        let nchunks = Nchunks::try_from(0);
        assert!(nchunks.is_err());

        let nchunks = Nchunks::try_from(-10);
        assert!(nchunks.is_err());
    }
}
