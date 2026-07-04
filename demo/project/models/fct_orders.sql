SELECT
    o.order_id,
    o.customer_id,
    o.order_date,
    o.status,
    o.amount,
    COALESCE(p.payment_method, 'unknown') AS payment_method,
    COALESCE(p.payment_status, 'pending') AS payment_status,
    CURRENT_TIMESTAMP AS loaded_at
FROM {{ ref('stg_orders') }} o
LEFT JOIN {{ ref('stg_payments') }} p ON o.order_id = p.order_id
