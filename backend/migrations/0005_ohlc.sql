-- Capture full OHLC (was close-only) for candlestick charts + valuation.
ALTER TABLE prices ADD COLUMN open REAL;
ALTER TABLE prices ADD COLUMN high REAL;
ALTER TABLE prices ADD COLUMN low REAL;
