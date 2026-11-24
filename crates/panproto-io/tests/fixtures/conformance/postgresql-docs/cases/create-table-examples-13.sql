CREATE TABLE circles (
    c circle,
    EXCLUDE USING gist (c WITH &&)
);
