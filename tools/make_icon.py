from pathlib import Path

from PIL import Image, ImageDraw


root = Path(__file__).resolve().parents[1]
output = root / "assets" / "app.ico"
output.parent.mkdir(parents=True, exist_ok=True)

scale = 4
size = 256
canvas = Image.new("RGBA", (size * scale, size * scale), (0, 0, 0, 0))
draw = ImageDraw.Draw(canvas)

def box(values):
    return tuple(int(v * scale) for v in values)

draw.rounded_rectangle(box((14, 45, 242, 211)), radius=55 * scale, fill="#121720", outline="#3a4352", width=4 * scale)
colors = ("#ff4d5f", "#ffbd2e", "#28d17c")
for x, color in zip((68, 128, 188), colors):
    draw.ellipse(box((x - 28, 100 - 28, x + 28, 100 + 28)), fill="#242c38")
    draw.ellipse(box((x - 21, 100 - 21, x + 21, 100 + 21)), fill=color)
    draw.ellipse(box((x - 12, 88, x - 2, 98)), fill=(255, 255, 255, 185))

canvas = canvas.resize((size, size), Image.Resampling.LANCZOS)
canvas.save(output, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print(output)
