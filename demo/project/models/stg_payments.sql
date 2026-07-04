SELECT
    id AS payment_id,
    order_id,
    payment_method,
    amount,
    status AS payment_status,
    CURRENT_TIMESTAMP AS loaded_at
FROM {{ source('raw', 'payments') }}
