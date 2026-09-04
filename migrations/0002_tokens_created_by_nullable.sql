-- Tokens made from the terminal have no person behind them; NULL says so.
ALTER TABLE api_tokens ALTER COLUMN created_by DROP NOT NULL;
