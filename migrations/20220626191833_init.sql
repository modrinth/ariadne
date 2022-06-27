CREATE TABLE downloads (
    id serial PRIMARY KEY,
    recorded timestamptz DEFAULT CURRENT_TIMESTAMP NOT NULL,
    downloads int NOT NULL,

    project_id varchar(11) NOT NULL,
    site_path varchar(1024) NOT NULL
);

CREATE TABLE revenue (
   id serial PRIMARY KEY,
   recorded timestamptz DEFAULT CURRENT_TIMESTAMP NOT NULL,
   money real NOT NULL,

   project_id varchar(11) NOT NULL
);

CREATE TABLE views (
    id serial PRIMARY KEY,
    recorded timestamptz DEFAULT CURRENT_TIMESTAMP NOT NULL,
    views int NOT NULL,

    project_id varchar(11) NOT NULL,
    site_path varchar(1024) NOT NULL
);