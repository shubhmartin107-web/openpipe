SELECT
    c.customer_id,
    c.first_name,
    c.last_name,
    c.email,
    c.signup_date,
    COALESCE(
        (SELECT MAX(order_date) FROM {{ ref('stg_orders') }} o WHERE o.customer_id = c.customer_id),
        c.signup_date
    ) AS last_order_date,
    DATEDIFF(day, c.signup_date, CURRENT_TIMESTAMP) AS customer_tenure_days,
    CASE
        WHEN (SELECT COUNT(*) FROM {{ ref('stg_orders') }} o WHERE o.customer_id = c.customer_id) = 0 THEN 'new'
        WHEN (SELECT COUNT(*) FROM {{ ref('stg_orders') }} o WHERE o.customer_id = c.customer_id) = 1 THEN 'one-time'
        ELSE 'repeat'
    END AS customer_type
FROM {{ ref('stg_customers') }} c
