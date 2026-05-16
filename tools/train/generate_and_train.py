import csv, random, subprocess
from pathlib import Path

random.seed(42)
rows = []
tick = 10_000_000

for _ in range(60000):
    pid_class = random.choices(
        [(4,1),(5,2),(6,4),(7,1),(8,2),(9,1),(10,3)],
        weights=[10, 8, 3, 8, 6, 40, 20],
        k=1
    )[0]
    pid, cls = pid_class

    intent_ids = {
        4: [100,101],
        5: [100,101],
        6: [100,101],
        7: [100,101,103],
        8: [100,112],
        9: [100,101,102,103,104,105,107,108,109],
        10: [100,272]
    }
    intent_id = random.choice(intent_ids[pid])
    anomaly = 0 if cls == 3 else random.randint(0, 5)
    tick += random.randint(100_000, 2_000_000)
    rows.append(f"T,{pid},{intent_id},{cls},{tick},{anomaly}")

try:
    with open("telemetry.csv") as f:
        for line in f:
            line = line.strip()
            if line.startswith("T,"):
                rows.append(line)
except FileNotFoundError:
    pass

with open("telemetry.csv", "w") as f:
    f.write("\n".join(rows) + "\n")

classes = {}
for r in rows:
    parts = r.split(",")
    if len(parts) >= 4:
        c = parts[3]
        classes[c] = classes.get(c, 0) + 1
print(f"Written {len(rows)} rows to telemetry.csv")
print("Class counts:", classes)

subprocess.run(
    ["python3", "tools/train/maya_train.py"],
    check=True
)
