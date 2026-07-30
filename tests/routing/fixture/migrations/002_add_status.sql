ALTER TABLE orders ADD COLUMN status text NOT NULL DEFAULT 'pending';
UPDATE orders SET status = 'legacy' WHERE created_at < NOW();
