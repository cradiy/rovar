ALTER TABLE objects DROP CONSTRAINT objects_kind_check;
ALTER TABLE objects ADD CONSTRAINT objects_kind_check
    CHECK (kind IN ('document', 'component', 'color_style'));
