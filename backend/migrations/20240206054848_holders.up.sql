CREATE TABLE holders (
  mint varchar(44) NOT NULL PRIMARY KEY,
  num bigint NOT NULL,
  created_at timestampz DEFAULT CURRENT_TIMESTAMP,
  updated_at timestampz DEFAULT CURRENT_TIMESTAMP
);
