-- 详情随请求删除，独立于列表查询，避免分页加载大正文。
CREATE TABLE IF NOT EXISTS request_details (
 request_log_id INTEGER PRIMARY KEY REFERENCES request_logs(id) ON DELETE CASCADE,
 payload TEXT NOT NULL
);
