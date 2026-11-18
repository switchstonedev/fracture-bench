pub use fracture_pacer::workload::*;

#[cfg(test)]
mod tests {
    use super::*;
    use fracture_core::types::QueryType;

    #[test]
    fn test_workload_ratios_and_vector_sanitization() {
        let mut gen = WorkloadGenerator::new_churn_70_20_10(128, 999);
        let mut searches = 0;
        let mut upserts = 0;
        let mut deletes = 0;

        for _ in 0..10_000 {
            match gen.next_op() {
                QueryType::Search { .. } | QueryType::AuditSearch { .. } => searches += 1,
                QueryType::Upsert { .. } => upserts += 1,
                QueryType::Delete { .. } => deletes += 1,
            }
        }

        let total = 10_000.0;
        let search_pct = (searches as f64) / total;
        let upsert_pct = (upserts as f64) / total;
        let delete_pct = (deletes as f64) / total;

        assert!(
            (search_pct - 0.70).abs() < 0.03,
            "Search ratio should be ~70%"
        );
        assert!(
            (upsert_pct - 0.20).abs() < 0.03,
            "Upsert ratio should be ~20%"
        );
        assert!(
            (delete_pct - 0.10).abs() < 0.02,
            "Delete ratio should be ~10%"
        );

        let vec = gen.generate_vector();
        assert_eq!(vec.len(), 128);
        for x in vec {
            assert!(!x.is_nan() && !x.is_infinite());
        }
    }
}
