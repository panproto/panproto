CREATE TABLE products (
    product_no integer,
    name text,
    price numeric
);
INSERT INTO products (product_no, name, price) VALUES (1, 'Cheese', DEFAULT);
INSERT INTO products DEFAULT VALUES;
