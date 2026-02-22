#[derive(Clone, sqlx::FromRow)]
pub struct TableDetails {
    pub name: String,
    pub total_size: i64,
    pub indexes_size: i64,
    pub approximate_row_count: i64,
}
