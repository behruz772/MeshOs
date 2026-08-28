# MeshOS

> Uzoq muddatli shaxsiy operatsion tizim loyihasi.

MeshOS — operatsion tizimni noldan yaratish, uning ichki ishlashini tushunish va bosqichma-bosqich rivojlantirish uchun boshlangan shaxsiy loyiha.

Bu repository faqat kodlar to‘plami emas. Unda MeshOS’ni yaratish davomida bo‘lgan g‘oyalar, tajribalar, xatolar, tuzatishlar va rivojlanish bosqichlari saqlanadi.

Agar men bu loyihaga yillar o‘tib qaytsam, ushbu README menga qayerdan boshlaganimni, nimani yaratishga harakat qilganimni va qanchalik rivojlanganligimni eslatishi kerak.

---

## Loyiha holati

**Hozirgi bosqich:** Faol ishlab chiqilmoqda

**Hozirgi checkpoint:** MeshOS v0.1 — Android va Control Center

**Checkpoint commit:** `22936f4`

**Branch:** `meshos-android-v1`

**Ishlash muhiti:** Linux / WSL + VirtualBox

**Boshlangan yil:** 2026

---

## Maqsad

MeshOS’ning uzoq muddatli maqsadi — o‘z operatsion tizimimizni yaratish va vaqt o‘tishi bilan uning asosiy qismlarini rivojlantirish:

- tizim komponentlari
- desktop muhit
- Control Center
- networking
- hardware support
- mobil integratsiya
- boshqa OS komponentlari

Loyiha bosqichma-bosqich ishlab chiqiladi.

### Rivojlantirish falsafasi

**Yarat → Sinab ko‘r → Buz → Tushun → Tuzat → Yaxshila**

Har bir xato va muammo loyihaning rivojlanish tarixining bir qismidir.

---

## Hozirgi komponentlar

Ushbu checkpoint’da loyiha quyidagilarni o‘z ichiga oladi:

- MeshOS core
- MeshOS daemon
- Workspace komponentlari
- Control Center
- MeshOS shell
- Android / mobile integratsiyasi
- Linux root filesystem
- maxsus boot/init konfiguratsiyasi
- VirtualBox test muhiti
- custom kernel integratsiyasi

---

## Android / Mobile

Loyihada MeshOS funksiyalari bilan ishlash uchun Android komponenti mavjud.

Asosiy fayllar:

```text
android/
mesh-core/src/mobile.rs
