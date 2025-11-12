--
-- PostgreSQL database dump
--

\restrict NVFOrpJrRKWxGgammaVrQTkvlPz8YOq1jzaQhhstNgvpojyJHAvt9BcdS297OX9

-- Dumped from database version 14.19 (Homebrew)
-- Dumped by pg_dump version 14.19 (Homebrew)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

ALTER TABLE ONLY public.ingredients DROP CONSTRAINT ingredients_user_id_fkey1;
ALTER TABLE ONLY public.ingredients DROP CONSTRAINT ingredients_user_id_fkey;
ALTER TABLE ONLY public.ingredients DROP CONSTRAINT ingredients_recipe_id_fkey;
ALTER TABLE ONLY public.ingredients DROP CONSTRAINT ingredients_ocr_entry_id_fkey1;
ALTER TABLE ONLY public.ingredients DROP CONSTRAINT ingredients_ocr_entry_id_fkey;
DROP INDEX public.recipes_content_tsv_idx;
DROP INDEX public.ocr_entries_content_tsv_idx;
DROP INDEX public.ingredients_user_id_idx;
DROP INDEX public.ingredients_recipe_id_idx;
DROP INDEX public.ingredients_ocr_entry_id_idx;
ALTER TABLE ONLY public.users DROP CONSTRAINT users_telegram_id_key;
ALTER TABLE ONLY public.users DROP CONSTRAINT users_pkey;
ALTER TABLE ONLY public.schema_migrations DROP CONSTRAINT schema_migrations_pkey;
ALTER TABLE ONLY public.recipes DROP CONSTRAINT recipes_pkey;
ALTER TABLE ONLY public.ocr_entries DROP CONSTRAINT ocr_entries_pkey;
ALTER TABLE ONLY public.ingredients DROP CONSTRAINT ingredients_pkey;
ALTER TABLE public.users ALTER COLUMN id DROP DEFAULT;
ALTER TABLE public.recipes ALTER COLUMN id DROP DEFAULT;
ALTER TABLE public.ocr_entries ALTER COLUMN id DROP DEFAULT;
ALTER TABLE public.ingredients ALTER COLUMN id DROP DEFAULT;
DROP SEQUENCE public.users_id_seq;
DROP TABLE public.users;
DROP TABLE public.schema_migrations;
DROP SEQUENCE public.recipes_id_seq;
DROP TABLE public.recipes;
DROP SEQUENCE public.ocr_entries_id_seq;
DROP TABLE public.ocr_entries;
DROP SEQUENCE public.ingredients_id_seq;
DROP TABLE public.ingredients;
SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: ingredients; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ingredients (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    ocr_entry_id bigint,
    name character varying(255) NOT NULL,
    quantity numeric(10,3),
    unit character varying(50),
    raw_text text NOT NULL,
    recipe_name character varying(255),
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    recipe_id bigint
);


--
-- Name: ingredients_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ingredients_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ingredients_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ingredients_id_seq OWNED BY public.ingredients.id;


--
-- Name: ocr_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ocr_entries (
    id bigint NOT NULL,
    telegram_id bigint NOT NULL,
    content text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english'::regconfig, content)) STORED
);


--
-- Name: ocr_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ocr_entries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ocr_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ocr_entries_id_seq OWNED BY public.ocr_entries.id;


--
-- Name: recipes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recipes (
    id bigint NOT NULL,
    telegram_id bigint NOT NULL,
    content text NOT NULL,
    recipe_name character varying(255),
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english'::regconfig, content)) STORED
);


--
-- Name: recipes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.recipes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: recipes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.recipes_id_seq OWNED BY public.recipes.id;


--
-- Name: schema_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.schema_migrations (
    version integer NOT NULL,
    name text NOT NULL,
    applied_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id bigint NOT NULL,
    telegram_id bigint NOT NULL,
    language_code character varying(10) DEFAULT 'en'::character varying,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- Name: ingredients id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients ALTER COLUMN id SET DEFAULT nextval('public.ingredients_id_seq'::regclass);


--
-- Name: ocr_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ocr_entries ALTER COLUMN id SET DEFAULT nextval('public.ocr_entries_id_seq'::regclass);


--
-- Name: recipes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recipes ALTER COLUMN id SET DEFAULT nextval('public.recipes_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- Name: ingredients ingredients_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients
    ADD CONSTRAINT ingredients_pkey PRIMARY KEY (id);


--
-- Name: ocr_entries ocr_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ocr_entries
    ADD CONSTRAINT ocr_entries_pkey PRIMARY KEY (id);


--
-- Name: recipes recipes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recipes
    ADD CONSTRAINT recipes_pkey PRIMARY KEY (id);


--
-- Name: schema_migrations schema_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.schema_migrations
    ADD CONSTRAINT schema_migrations_pkey PRIMARY KEY (version);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_telegram_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_telegram_id_key UNIQUE (telegram_id);


--
-- Name: ingredients_ocr_entry_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ingredients_ocr_entry_id_idx ON public.ingredients USING btree (ocr_entry_id);


--
-- Name: ingredients_recipe_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ingredients_recipe_id_idx ON public.ingredients USING btree (recipe_id);


--
-- Name: ingredients_user_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ingredients_user_id_idx ON public.ingredients USING btree (user_id);


--
-- Name: ocr_entries_content_tsv_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ocr_entries_content_tsv_idx ON public.ocr_entries USING gin (content_tsv);


--
-- Name: recipes_content_tsv_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX recipes_content_tsv_idx ON public.recipes USING gin (content_tsv);


--
-- Name: ingredients ingredients_ocr_entry_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients
    ADD CONSTRAINT ingredients_ocr_entry_id_fkey FOREIGN KEY (ocr_entry_id) REFERENCES public.ocr_entries(id);


--
-- Name: ingredients ingredients_ocr_entry_id_fkey1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients
    ADD CONSTRAINT ingredients_ocr_entry_id_fkey1 FOREIGN KEY (ocr_entry_id) REFERENCES public.ocr_entries(id);


--
-- Name: ingredients ingredients_recipe_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients
    ADD CONSTRAINT ingredients_recipe_id_fkey FOREIGN KEY (recipe_id) REFERENCES public.recipes(id);


--
-- Name: ingredients ingredients_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients
    ADD CONSTRAINT ingredients_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: ingredients ingredients_user_id_fkey1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingredients
    ADD CONSTRAINT ingredients_user_id_fkey1 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- PostgreSQL database dump complete
--

\unrestrict NVFOrpJrRKWxGgammaVrQTkvlPz8YOq1jzaQhhstNgvpojyJHAvt9BcdS297OX9

