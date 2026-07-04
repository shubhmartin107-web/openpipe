SELECT
    id AS order_id,
    customer_id,
    order_date,
    status,
    amount,
    CURRENT_TIMESTAMP AS loaded_at
FROM {{ source('raw', 'orders') }}
WHERE status IS NOT NULL
