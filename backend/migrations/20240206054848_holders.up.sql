CREATE TABLE holders (
  mint varchar(44) NOT NULL PRIMARY KEY,
  num bigint NOT NULL,
  created_at  timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
  updated_at  timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);
