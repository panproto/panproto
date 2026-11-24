CREATE TABLE distributors (
    did     integer CHECK (did > 100),
    name    varchar(40)
);
