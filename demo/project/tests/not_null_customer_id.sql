SELECT customer_id
FROM {{ ref('dim_customers') }}
WHERE customer_id IS NULL
