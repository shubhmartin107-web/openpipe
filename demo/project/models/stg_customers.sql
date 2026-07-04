SELECT
    id AS customer_id,
    first_name,
    last_name,
    email,
    signup_date,
    CURRENT_TIMESTAMP AS loaded_at
FROM {{ source('raw', 'customers') }}
