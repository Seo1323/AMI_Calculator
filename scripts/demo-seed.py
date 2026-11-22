"""Fills an AMI_Calculator ami.db with fictional demo data (used for the README screenshots).

usage: python3 -I scripts/demo-seed.py <ami.db> <light|dark> [locale]
Run the app once with XDG_DATA_HOME etc. pointing at a temp dir to create the DB, close it, seed, relaunch.
"""
import json, sqlite3, sys, uuid, datetime as dt

db, theme = sys.argv[1], sys.argv[2]
locale = sys.argv[3] if len(sys.argv) > 3 else "pl"
now = dt.datetime.now(dt.timezone.utc)
TS = now.strftime("%Y-%m-%dT%H:%M:%S.000000Z")
con = sqlite3.connect(db)


def put(table, data, extra=None):
    i = str(uuid.uuid4())
    data = {"id": i, "createdAt": TS, "updatedAt": TS, **data}
    con.execute(
        f"INSERT INTO {table} (id, created_at, updated_at, deleted_at, data) VALUES (?,?,?,NULL,?)",
        (i, TS, TS, json.dumps(data)),
    )
    return i


def inst(symbol, name, kind, cur, src, price, frac=False, **kw):
    i = put("instruments", dict(symbol=symbol, name=name, kind=kind, currency=cur,
                                quoteSource=src, fractional=frac, **kw))
    q = dict(instrumentId=i, price=str(price), currency=cur, source="manual", fetchedAt=TS)
    q.update(kw.pop("quote", {}))
    con.execute("INSERT INTO quotes (instrument_id, data, fetched_at) VALUES (?,?,?)", (i, json.dumps(q), TS))
    return i


vwce = inst("VWCE.DE", "Vanguard FTSE All-World UCITS ETF", "etf", "EUR", "yahoo", "135.20")
bw20 = inst("ETFBW20TR", "Beta ETF WIG20TR", "etf", "PLN", "yahoo", "132.40")
edo = inst("EDO1135", "Obligacje skarbowe EDO1135", "bond", "PLN", "manual", "104.50",
           interestPercent="5.60", capitalization="annual", startDate="2025-11-01", maturityDate="2035-11-01")
btc = inst("bitcoin", "Bitcoin", "crypto", "USD", "coinGecko", "94000", frac=True)
cash = inst("PLN", "Gotówka PLN", "cash", "PLN", "manual", "1")
aapl = inst("AAPL", "Apple Inc.", "stock", "USD", "yahoo", "225.00")
sxr8 = inst("SXR8.DE", "iShares Core S&P 500 UCITS ETF", "etf", "EUR", "yahoo", "560.00")
lok = inst("LOK6M", "Lokata 6 miesięcy", "deposit", "PLN", "manual", "1",
           interestPercent="5.00", capitalization="monthly", startDate="2026-09-01", maturityDate="2027-03-01")

for cur, rate in (("EUR", "4.2500"), ("USD", "3.6500")):
    con.execute("INSERT INTO fx_rates (currency, data, fetched_at) VALUES (?,?,?)",
                (cur, json.dumps(dict(currency=cur, ratePln=rate, source="manual", fetchedAt=TS)), TS))

ike = put("portfolios", dict(name="IKE – długoterminowy", baseCurrency="PLN", tolerancePercent="5", accountType="ike"))
brk = put("portfolios", dict(name="Konto maklerskie", baseCurrency="PLN", tolerancePercent="5", accountType="regular"))


def target(p, i, pct):
    put("allocation_targets", dict(portfolioId=p, instrumentId=i, targetPercent=str(pct)))


for i, pct in ((vwce, 50), (bw20, 15), (edo, 25), (cash, 10)):
    target(ike, i, pct)
for i, pct in ((aapl, 25), (sxr8, 40), (btc, 20), (lok, 15)):
    target(brk, i, pct)


def trade(p, i, date, side, qty, price):
    put("trades", dict(portfolioId=p, instrumentId=i, date=date, side=side, quantity=str(qty),
                       price=str(price), fee="0"))


for d, q, pr in (("2025-03-10", 12, "118.00"), ("2025-09-15", 8, "128.00"), ("2026-04-02", 5, "126.00")):
    trade(ike, vwce, d, "buy", q, pr)
trade(ike, bw20, "2025-05-12", "buy", 40, "118.00")
trade(ike, edo, "2025-11-03", "buy", 60, "100.00")
trade(ike, cash, "2025-03-01", "buy", 2000, "1")
trade(brk, aapl, "2025-01-15", "buy", 10, "170.00")
trade(brk, aapl, "2025-08-20", "buy", 8, "185.00")
trade(brk, aapl, "2026-06-10", "sell", 3, "210.00")
trade(brk, sxr8, "2025-02-11", "buy", 8, "520.00")
trade(brk, btc, "2025-06-05", "buy", "0.04", "88000")
trade(brk, lok, "2026-09-01", "buy", 5000, "1")

# value history: slow climb with some wobble, monthly
y, m, v = 2025, 3, 3800.0
wob = [0, 400, -250, 900, 300, -500, 1200, 700, 200, 1500, -300, 900, 1100, 600, 1000, 800, 1400, 900, 700, 1000]
for k, w in enumerate(wob):
    day = f"{y}-{m:02d}-28"
    con.execute("INSERT OR REPLACE INTO value_history VALUES (?,?,?,?)", (ike, day, "PLN", f"{v + w:.2f}"))
    v += 1150
    m += 1
    if m > 12:
        m, y = 1, y + 1

ui = dict(theme=theme, locale=locale, autoRefreshQuotes=False, startPage="/", sidebarCollapsed=False)
con.execute("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES ('ui', ?, ?)", (json.dumps(ui), TS))
con.commit()
print("seeded", db, theme)
