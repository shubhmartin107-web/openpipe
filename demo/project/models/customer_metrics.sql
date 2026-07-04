SELECT
    c.customer_id,
    c.first_name,
    c.last_name,
    c.customer_type,
    c.customer_tenure_days,
    COUNT(DISTINCT f.order_id) AS total_orders,
    COALESCE(SUM(f.amount), 0) AS total_revenue,
    COALESCE(AVG(f.amount), 0) AS avg_order_value,
    MIN(f.order_date) AS first_order_date,
    MAX(f.order_date) AS last_order_date,
    CASE
        WHEN COUNT(DISTINCT f.order_id) = 0 THEN 0
        ELSE COALESCE(SUM(f.amount), 0) / NULLIF(COUNT(DISTINCT f.order_id), 0)
    END AS revenue_per_order
FROM {{ ref('dim_customers') }} c
LEFT JOIN {{ ref('fct_orders') }} f ON c.customer_id = f.customer_id
GROUP BY 1, 2, 3, 4, 5
