CREATE TABLE products (
    product_no integer,
    name text,
    price numeric
);
INSERT INTO products (product_no, name, price)
  SELECT product_no, name, price FROM new_products
    WHERE release_date = 'today';
