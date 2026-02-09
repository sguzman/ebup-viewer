-- strip-nontext.lua
-- Aggressive cleanup for TTS-oriented plain text conversion.

local skipping_toc = false

local function trim(s)
  s = s:gsub("^%s+", "")
  s = s:gsub("%s+$", "")
  return s
end

local function normalize_label(s)
  return trim(s):gsub("%s+", " "):upper()
end

local function is_toc_label(s)
  local t = normalize_label(s)
  return t == "CONTENTS"
      or t == "TABLE OF CONTENTS"
      or t == "ILLUSTRATIONS"
      or t == "LIST OF ILLUSTRATIONS"
      or t == "LIST OF FIGURES"
end

local function is_ascii_border_line(s)
  s = trim(s)
  return s:match("^%+[-=+]+%+$") ~= nil
end

local function is_ascii_table_row(s)
  s = trim(s)
  return s:match("^%|.*%|$") ~= nil
end

local function looks_like_inflating_ascii_rule(s)
  s = trim(s)
  if #s < 120 then
    return false
  end

  local rule_chars = select(2, s:gsub("[-=+|]", ""))
  local letters = select(2, s:gsub("%a", ""))
  return rule_chars > (#s * 0.9) and letters < 8
end

local function looks_like_toc_entry(s)
  s = trim(s)
  if s == "" then
    return false
  end
  if s:match("^.+[%.][%.%. ]+[0-9ivxlcdmIVXLCDM]+$") then
    return true
  end
  if s:match("^%d+%.%s+.+$") then
    return true
  end
  return false
end

local function is_stub_marker(s)
  local t = normalize_label(s)
  return t == "[IMAGE]"
      or t == "[FIGURE]"
      or t == "[TABLE]"
      or t == "IMAGE"
      or t == "FIGURE"
      or t == "TABLE"
end

local function should_drop_ascii_tableish_para(s)
  return is_ascii_border_line(s)
      or is_ascii_table_row(s)
      or looks_like_inflating_ascii_rule(s)
end

local function process_block(block)
  local s = pandoc.utils.stringify(block)
  local t = trim(s)

  if t == "" then
    if skipping_toc then
      return {}
    end
    return block
  end

  if is_toc_label(t) then
    skipping_toc = true
    return {}
  end

  if is_stub_marker(t) then
    return {}
  end

  if skipping_toc then
    if should_drop_ascii_tableish_para(t) or looks_like_toc_entry(t) then
      return {}
    end
    skipping_toc = false
  end

  if should_drop_ascii_tableish_para(t) then
    return {}
  end

  return block
end

function Header(h)
  local s = pandoc.utils.stringify(h)
  if is_toc_label(s) then
    skipping_toc = true
    return {}
  end
  return h
end

function Para(p)
  return process_block(p)
end

function Plain(p)
  return process_block(p)
end

-- Drop real non-text structures.
function Table(_)
  return {}
end

function Figure(_)
  return {}
end

function Image(_)
  return {}
end

function Note(_)
  return {}
end

function RawBlock(_)
  return {}
end

function RawInline(_)
  return {}
end

function Math(_)
  return {}
end

-- Keep visible link text and drop URL target.
function Link(l)
  return l.content
end

-- Flatten containers.
function Div(d)
  return d.content
end

function Span(s)
  return s.content
end

-- Remove literal marker tokens if they survive parsing.
function Str(el)
  local t = normalize_label(el.text)
  if t == "[IMAGE]" or t == "[FIGURE]" or t == "[TABLE]" then
    return {}
  end
  return el
end
