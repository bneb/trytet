import random
import sys

# We run 100,000 simulations
simulations = 100000

print(f"Running Monte Carlo Simulation for Trytet Infrastructure Exit ({simulations} iterations)...\n")

prob_burnout = 0.05
prob_acquihire = 0.15
prob_seed = 0.30
prob_series_a = 0.35
prob_unicorn = 0.15

results = []
for i in range(simulations):
    rand = random.random()
    if rand < prob_burnout:
        results.append((0.0, "Burnout / Technical Fail"))
    elif rand < prob_burnout + prob_acquihire:
        results.append((random.gauss(10, 3), "Acquihire"))
    elif rand < prob_burnout + prob_acquihire + prob_seed:
        results.append((random.gauss(30, 5), "Seed Stabilization"))
    elif rand < prob_burnout + prob_acquihire + prob_seed + prob_series_a:
        results.append((random.gauss(100, 20), "Series A Growth"))
    else:
        results.append((random.gauss(1200, 300), "Decacorn/Unicorn Exit"))

values = sorted([r[0] for r in results])
import math

def percentile(data, perc):
    k = (len(data)-1) * (perc/100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c: return data[int(k)]
    d0 = data[int(f)] * (c-k)
    d1 = data[int(c)] * (k-f)
    return d0+d1

mean_val = sum(values)/len(values)
median_val = percentile(values, 50)
p5 = percentile(values, 5)
p95 = percentile(values, 95)
max_val = values[-1]

print("==========================================================================")
print("Monte Carlo Exit Valuation (Post-Remediation Phase 12.5)")
print("==========================================================================")
print(f"Mean Expected Exit       | ${mean_val:,.2f}M")
print(f"Median Exit              | ${median_val:,.2f}M")
print(f"5th Percentile (Downside)| ${p5:,.2f}M")
print(f"95th Percentile (Upside) | ${p95:,.2f}M")
print(f"Max Outlier              | ${max_val:,.2f}M")
print("==========================================================================")
