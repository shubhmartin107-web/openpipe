-- Seed data for the retail demo
-- Run this against the lakehouse SQL engine (Spark/Trino) to create raw tables

CREATE DATABASE IF NOT EXISTS raw;
USE raw;

CREATE TABLE IF NOT EXISTS raw.customers (
  id INT,
  first_name STRING,
  last_name STRING,
  email STRING,
  signup_date DATE
) USING iceberg;

INSERT INTO raw.customers VALUES
  (1, 'John', 'Doe', 'john@example.com', '2024-01-15'),
  (2, 'Jane', 'Smith', 'jane@example.com', '2024-02-20'),
  (3, 'Bob', 'Johnson', 'bob@example.com', '2024-03-10'),
  (4, 'Alice', 'Williams', 'alice@example.com', '2024-04-05'),
  (5, 'Charlie', 'Brown', 'charlie@example.com', '2024-05-01'),
  (6, 'Diana', 'Prince', 'diana@example.com', '2024-06-15'),
  (7, 'Edward', 'Norton', 'edward@example.com', '2024-07-22'),
  (8, 'Fiona', 'Apple', 'fiona@example.com', '2024-08-30');

CREATE TABLE IF NOT EXISTS raw.orders (
  id INT,
  customer_id INT,
  order_date DATE,
  status STRING,
  amount DECIMAL(10,2)
) USING iceberg;

INSERT INTO raw.orders VALUES
  (101, 1, '2024-06-01', 'completed', 150.00),
  (102, 1, '2024-07-15', 'completed', 200.00),
  (103, 2, '2024-06-10', 'completed', 75.50),
  (104, 3, '2024-07-01', 'shipped', 300.00),
  (105, 3, '2024-08-15', 'placed', 450.00),
  (106, 4, '2024-08-01', 'completed', 125.00),
  (107, 5, '2024-09-01', 'placed', 600.00),
  (108, 1, '2024-09-10', 'returned', 200.00),
  (109, 6, '2024-09-15', 'shipped', 350.00),
  (110, 7, '2024-10-01', 'completed', 80.00);

CREATE TABLE IF NOT EXISTS raw.payments (
  id INT,
  order_id INT,
  payment_method STRING,
  amount DECIMAL(10,2),
  status STRING
) USING iceberg;

INSERT INTO raw.payments VALUES
  (1001, 101, 'credit_card', 150.00, 'completed'),
  (1002, 102, 'paypal', 200.00, 'completed'),
  (1003, 103, 'credit_card', 75.50, 'completed'),
  (1004, 104, 'bank_transfer', 300.00, 'pending'),
  (1005, 106, 'credit_card', 125.00, 'completed'),
  (1006, 107, 'paypal', 600.00, 'pending'),
  (1007, 108, 'credit_card', 200.00, 'completed'),
  (1008, 109, 'credit_card', 350.00, 'pending'),
  (1009, 110, 'debit_card', 80.00, 'completed');
