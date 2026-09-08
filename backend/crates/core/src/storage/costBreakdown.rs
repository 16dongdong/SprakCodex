use super::Storage;
use serde::Serialize;

// 金额来自历史请求价格快照；缓存写入归入缓存项，避免用当前价格重算过去费用。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    input: f64,
    output: f64,
    cache: f64,
    total: f64,
}

impl Storage {
    // 全量累计在 SQL 中聚合，价格单位为 micro-USD/百万 Token；先转 REAL 避免整数乘法溢出。
    // 输出与缓存先按快照单价计算，微美元舍入尾差归入输入，确保分项总和等于历史快照总额。
    #[allow(non_snake_case)]
    pub fn cumulativeCostBreakdown(&self) -> rusqlite::Result<CostBreakdown> {
        self.conn.query_row(
            "SELECT COALESCE(SUM(base_cost_microusd / 1000000.0),0),
                    COALESCE(SUM(output_tokens * 1.0 * output_microusd_per_1m / 1000000000000.0),0),
                    COALESCE(SUM((cached_input_tokens * 1.0 * cached_input_microusd_per_1m +
                        cache_write_tokens * 1.0 * COALESCE(cache_write_microusd_per_1m,input_microusd_per_1m)) / 1000000000000.0),0)
             FROM request_charge_snapshots",
            [],
            |row| {
                let total: f64 = row.get(0)?;
                let output: f64 = row.get(1)?;
                let cache: f64 = row.get(2)?;
                Ok(CostBreakdown { input: (total - output - cache).max(0.0), output, cache, total })
            },
        )
    }
}
