#!/usr/bin/env python3
"""
Generate a beautiful app icon for Pyr Reader featuring a Great Pyrenees dog.
Creates a 1024x1024 PNG with a stylized, friendly dog face on a purple-blue gradient.
"""

from PIL import Image, ImageDraw, ImageFilter
import math


def create_gradient_background(size=1024):
    """Create a rounded-rect background with purple-blue gradient."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # Gradient colors: #667eea to #764ba2
    r1, g1, b1 = 0x66, 0x7E, 0xEA
    r2, g2, b2 = 0x76, 0x4B, 0xA2

    for y in range(size):
        for x in range(size):
            t = (x / size * 0.4 + y / size * 0.6)
            t = max(0, min(1, t))
            r = int(r1 + (r2 - r1) * t)
            g = int(g1 + (g2 - g1) * t)
            b = int(b1 + (b2 - b1) * t)
            img.putpixel((x, y), (r, g, b, 255))

    # Rounded rect mask
    corner_radius = int(size * 0.22)
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(
        [(0, 0), (size - 1, size - 1)],
        radius=corner_radius,
        fill=255,
    )
    img.putalpha(mask)
    return img


def fluffy_circle(draw, cx, cy, base_radius, color, bumps=10, bump_ratio=0.22):
    """Draw a fluffy/cloudy circle shape with bumpy edges."""
    # Draw center
    draw.ellipse(
        [cx - base_radius, cy - base_radius, cx + base_radius, cy + base_radius],
        fill=color,
    )
    # Draw bumps around the perimeter
    for i in range(bumps):
        angle = (2 * math.pi * i) / bumps
        bx = cx + int(base_radius * 0.75 * math.cos(angle))
        by = cy + int(base_radius * 0.75 * math.sin(angle))
        br = int(base_radius * (0.55 + bump_ratio))
        draw.ellipse([bx - br, by - br, bx + br, by + br], fill=color)


def create_dog_face(size=1024):
    """Draw the Great Pyrenees face."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    cx = size // 2
    cy = size // 2 + 30

    # -- Colors --
    white_fur = (252, 250, 247, 255)
    cream_light = (247, 242, 234, 255)
    cream = (238, 230, 215, 255)
    cream_shadow = (225, 215, 198, 255)
    ear_color = (228, 216, 196, 255)
    ear_inner = (215, 200, 178, 255)
    ear_dark = (200, 185, 162, 255)
    eye_socket = (215, 205, 190, 255)
    eye_dark = (40, 30, 25, 255)
    eye_brown = (85, 60, 40, 255)
    eye_warm = (105, 75, 50, 255)
    eye_shine = (255, 255, 255, 230)
    nose_black = (30, 25, 22, 255)
    nose_grey = (65, 55, 50, 255)
    mouth_line = (80, 60, 50, 255)
    tongue_pink = (235, 155, 145, 255)
    tongue_dark = (215, 125, 118, 255)

    # ============================================
    # EARS - floppy, visible on sides of head
    # ============================================
    # Left ear
    ear_pts_l = [
        (cx - 220, cy - 160),  # top attach
        (cx - 310, cy - 100),  # top outer
        (cx - 340, cy + 10),   # middle outer
        (cx - 320, cy + 130),  # bottom outer
        (cx - 270, cy + 170),  # bottom tip
        (cx - 230, cy + 130),  # bottom inner
        (cx - 210, cy + 20),   # middle inner
        (cx - 210, cy - 80),   # inner top
    ]
    draw.polygon(ear_pts_l, fill=ear_color)
    # Ear inner shading
    ear_inner_l = [
        (cx - 235, cy - 100),
        (cx - 300, cy - 60),
        (cx - 320, cy + 30),
        (cx - 300, cy + 110),
        (cx - 265, cy + 140),
        (cx - 240, cy + 100),
        (cx - 228, cy + 10),
        (cx - 225, cy - 50),
    ]
    draw.polygon(ear_inner_l, fill=ear_inner)
    # Darker fold line
    ear_fold_l = [
        (cx - 250, cy - 70),
        (cx - 290, cy - 30),
        (cx - 305, cy + 50),
        (cx - 290, cy + 100),
        (cx - 275, cy + 80),
        (cx - 280, cy + 30),
        (cx - 268, cy - 20),
        (cx - 245, cy - 50),
    ]
    draw.polygon(ear_fold_l, fill=ear_dark)

    # Right ear (mirror)
    ear_pts_r = [(2 * cx - x, y) for x, y in ear_pts_l]
    draw.polygon(ear_pts_r, fill=ear_color)
    ear_inner_r = [(2 * cx - x, y) for x, y in ear_inner_l]
    draw.polygon(ear_inner_r, fill=ear_inner)
    ear_fold_r = [(2 * cx - x, y) for x, y in ear_fold_l]
    draw.polygon(ear_fold_r, fill=ear_dark)

    # ============================================
    # NECK FLUFF / MANE (behind lower head)
    # ============================================
    for dx in range(-3, 4):
        nx = cx + dx * 65
        fluffy_circle(draw, nx, cy + 280, 85, cream_light, 8, 0.25)
    for dx in range(-2, 3):
        nx = cx + dx * 80
        fluffy_circle(draw, nx, cy + 250, 75, white_fur, 7, 0.2)

    # ============================================
    # HEAD - main shape
    # ============================================
    # Base head oval
    draw.ellipse(
        [cx - 280, cy - 250, cx + 280, cy + 200],
        fill=cream_light,
    )
    # Brighter inner area
    draw.ellipse(
        [cx - 250, cy - 220, cx + 250, cy + 170],
        fill=white_fur,
    )

    # ============================================
    # FOREHEAD FLUFF - top of head
    # ============================================
    for i in range(7):
        angle = math.pi + (math.pi * i) / 6  # top semicircle
        fx = cx + int(180 * math.cos(angle))
        fy = cy - 220 + int(60 * math.sin(angle))
        fluffy_circle(draw, fx, fy, 65, white_fur, 6, 0.2)

    # Center crown fluff
    fluffy_circle(draw, cx, cy - 270, 80, white_fur, 8, 0.25)
    fluffy_circle(draw, cx - 70, cy - 250, 55, cream_light, 6, 0.2)
    fluffy_circle(draw, cx + 70, cy - 250, 55, cream_light, 6, 0.2)

    # ============================================
    # CHEEK FLUFF
    # ============================================
    fluffy_circle(draw, cx - 240, cy + 30, 80, white_fur, 7, 0.25)
    fluffy_circle(draw, cx + 240, cy + 30, 80, white_fur, 7, 0.25)
    fluffy_circle(draw, cx - 220, cy + 80, 70, cream_light, 6, 0.2)
    fluffy_circle(draw, cx + 220, cy + 80, 70, cream_light, 6, 0.2)

    # ============================================
    # MUZZLE
    # ============================================
    muzzle_y = cy + 60
    # Muzzle bump
    draw.ellipse(
        [cx - 150, muzzle_y - 60, cx + 150, muzzle_y + 100],
        fill=cream_light,
    )
    draw.ellipse(
        [cx - 130, muzzle_y - 40, cx + 130, muzzle_y + 80],
        fill=white_fur,
    )
    # Two muzzle bumps (the puffy cheeks around nose)
    draw.ellipse(
        [cx - 110, muzzle_y - 10, cx - 5, muzzle_y + 60],
        fill=white_fur,
    )
    draw.ellipse(
        [cx + 5, muzzle_y - 10, cx + 110, muzzle_y + 60],
        fill=white_fur,
    )

    # ============================================
    # EYES - large, kind, almond-shaped
    # ============================================
    eye_y = cy - 50
    eye_sep = 120

    for side in [-1, 1]:
        ex = cx + side * eye_sep

        # Eye socket subtle shadow
        draw.ellipse(
            [ex - 55, eye_y - 42, ex + 55, eye_y + 42],
            fill=eye_socket,
        )
        draw.ellipse(
            [ex - 50, eye_y - 38, ex + 50, eye_y + 38],
            fill=cream_light,
        )

        # Outer eye shape (almond-ish via ellipse)
        draw.ellipse(
            [ex - 42, eye_y - 30, ex + 42, eye_y + 30],
            fill=eye_dark,
        )

        # Brown iris
        draw.ellipse(
            [ex - 32, eye_y - 22, ex + 32, eye_y + 22],
            fill=eye_brown,
        )

        # Warm inner iris
        draw.ellipse(
            [ex - 22, eye_y - 16, ex + 22, eye_y + 16],
            fill=eye_warm,
        )

        # Dark pupil
        draw.ellipse(
            [ex - 16, eye_y - 14, ex + 16, eye_y + 14],
            fill=eye_dark,
        )

        # Main highlight
        hx = ex + side * 12
        draw.ellipse(
            [hx - 11, eye_y - 16, hx + 11, eye_y - 4],
            fill=eye_shine,
        )
        # Secondary small highlight
        draw.ellipse(
            [ex - side * 10 - 5, eye_y + 6, ex - side * 10 + 5, eye_y + 14],
            fill=(255, 255, 255, 100),
        )

        # Eyelids - subtle curves above and below
        # Upper lid line
        draw.arc(
            [ex - 44, eye_y - 34, ex + 44, eye_y + 10],
            start=190, end=350,
            fill=cream_shadow,
            width=4,
        )

    # Eyebrow fur tufts
    for side in [-1, 1]:
        ex = cx + side * eye_sep
        brow_y = eye_y - 55
        for j in range(4):
            bx = ex - 25 + j * 17
            draw.ellipse(
                [bx - 14, brow_y - 8, bx + 14, brow_y + 8],
                fill=white_fur,
            )

    # ============================================
    # NOSE
    # ============================================
    nose_y = cy + 55
    # Main nose shape - rounded triangle
    nose_w, nose_h = 44, 36
    nose_pts = [
        (cx - nose_w, nose_y - 5),
        (cx - nose_w + 5, nose_y - nose_h + 5),
        (cx - 20, nose_y - nose_h - 5),
        (cx, nose_y - nose_h - 8),
        (cx + 20, nose_y - nose_h - 5),
        (cx + nose_w - 5, nose_y - nose_h + 5),
        (cx + nose_w, nose_y - 5),
        (cx + 30, nose_y + 12),
        (cx + 15, nose_y + 20),
        (cx, nose_y + 22),
        (cx - 15, nose_y + 20),
        (cx - 30, nose_y + 12),
    ]
    draw.polygon(nose_pts, fill=nose_black)

    # Nose bridge highlight
    draw.ellipse(
        [cx - 20, nose_y - nose_h + 2, cx + 20, nose_y - nose_h + 18],
        fill=nose_grey,
    )
    draw.ellipse(
        [cx - 12, nose_y - nose_h + 5, cx + 12, nose_y - nose_h + 14],
        fill=(90, 80, 72, 255),
    )

    # Nostrils
    draw.ellipse(
        [cx - 24, nose_y - 6, cx - 8, nose_y + 8],
        fill=(18, 14, 12, 255),
    )
    draw.ellipse(
        [cx + 8, nose_y - 6, cx + 24, nose_y + 8],
        fill=(18, 14, 12, 255),
    )

    # ============================================
    # MOUTH
    # ============================================
    mouth_top = nose_y + 22
    # Line from nose down
    draw.line(
        [(cx, mouth_top), (cx, mouth_top + 15)],
        fill=mouth_line,
        width=3,
    )
    # Smile arcs
    draw.arc(
        [cx - 75, mouth_top - 5, cx + 5, mouth_top + 40],
        start=0, end=55,
        fill=mouth_line,
        width=3,
    )
    draw.arc(
        [cx - 5, mouth_top - 5, cx + 75, mouth_top + 40],
        start=125, end=180,
        fill=mouth_line,
        width=3,
    )

    # Tongue
    tongue_y = mouth_top + 20
    draw.ellipse(
        [cx - 22, tongue_y - 2, cx + 22, tongue_y + 30],
        fill=tongue_pink,
    )
    draw.ellipse(
        [cx - 15, tongue_y + 5, cx + 15, tongue_y + 25],
        fill=tongue_dark,
    )
    draw.line(
        [(cx, tongue_y + 2), (cx, tongue_y + 24)],
        fill=(195, 105, 100, 255),
        width=2,
    )

    # ============================================
    # CHIN FLUFF
    # ============================================
    chin_y = cy + 175
    fluffy_circle(draw, cx, chin_y, 80, white_fur, 7, 0.22)
    fluffy_circle(draw, cx - 60, chin_y - 15, 55, cream_light, 5, 0.18)
    fluffy_circle(draw, cx + 60, chin_y - 15, 55, cream_light, 5, 0.18)

    return img


def add_depth(bg, dog, size=1024):
    """Add subtle shadow behind the dog and composite."""
    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    dog_alpha = dog.split()[3]
    shadow_color = Image.new("RGBA", (size, size), (30, 15, 50, 50))
    shadow.paste(shadow_color, mask=dog_alpha)
    shadow = shadow.filter(ImageFilter.GaussianBlur(radius=18))

    shadow_offset = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow_offset.paste(shadow, (0, 10))

    result = bg.copy()
    result = Image.alpha_composite(result, shadow_offset)
    result = Image.alpha_composite(result, dog)
    return result


def main():
    size = 1024
    print("Creating gradient background...")
    bg = create_gradient_background(size)

    print("Drawing Great Pyrenees face...")
    dog = create_dog_face(size)

    print("Compositing layers with depth...")
    icon = add_depth(bg, dog, size)

    # Light smoothing pass
    smooth = icon.filter(ImageFilter.SMOOTH_MORE)
    final = Image.blend(icon, smooth, 0.2)

    output_path = "/Users/ricardotrentin/Documents/2026/pyr-reader/app-icon.png"
    final.save(output_path, "PNG")
    print(f"Icon saved to {output_path}")
    print(f"Size: {final.size}")


if __name__ == "__main__":
    main()
