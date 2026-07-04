SELECT customer_id, COUNT(*)
FROM {{ ref('dim_customers') }}
GROUP BY customer_id
HAVING COUNT(*) > 1
