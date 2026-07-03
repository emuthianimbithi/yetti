CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    customer_name TEXT NOT NULL,
    total_amount NUMERIC(12, 2) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO orders (id, customer_name, total_amount, updated_at)
VALUES
    (1, 'Amina Foods', 125.50, '2026-07-01T09:00:00Z'),
    (2, 'Baraka Stores', 88.20, '2026-07-01T10:00:00Z'),
    (3, 'Coastal Supplies', 240.00, '2026-07-01T11:00:00Z');
