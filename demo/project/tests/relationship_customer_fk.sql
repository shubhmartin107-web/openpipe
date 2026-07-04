SELECT f.customer_id
FROM {{ ref('fct_orders') }} f
LEFT JOIN {{ ref('dim_customers') }} d ON f.customer_id = d.customer_id
WHERE d.customer_id IS NULL
