-- How the user's recipe cards show measures (UX plan U8): `unit_view::UnitView`
ALTER TABLE users ADD COLUMN IF NOT EXISTS unit_view TEXT NOT NULL DEFAULT 'french';
