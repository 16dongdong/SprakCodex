-- 名称来自本次客户端身份，仅用于展示，不参与账号池选路或授权。
ALTER TABLE request_logs ADD COLUMN account_label TEXT;
