# Próximamente en Forja-3D

Hoja de ruta de mejoras planificadas para **Forja-3D** (`forja3d`, `forja3d-physics` y `forja3d-studio`).

---

## 1. Gráficos, Iluminación y Post-procesado
- [ ] **Sombras en tiempo real (Shadow Mapping)** para los objetos **Sol**, con mapa de sombras direccional y suavizado (PCF).
- [x] **Shader PBR real (Cook-Torrance)** con Fresnel y respuesta física realista a `metallic`, `roughness` e intensidad del Sol.
- [ ] **Luces puntuales y focos (`PointLight` / `SpotLight`)** además del Sol (faroles, motores, antorchas, interiores).
- [ ] **Cielo atmosférico / HDRI procedural** ligado a la dirección e intensidad del Sol.
- [ ] **Post-procesado**: niebla atmosférica, **Bloom** (brillo en luces y emisivos) y **Anti-aliasing (MSAA)**.

---

## 2. Materiales, Texturas y Pintado 3D
- [ ] **Mapas de Normal, Roughness y Emissive** además de la textura base (*albedo*).
- [ ] **Tiling y Offset de UVs por pieza** (repetición, escala y desplazamiento para evitar estiramientos en modelos grandes).
- [ ] **Pincel de pintado 3D en el viewport** (pintar vértices o textura directamente con ratón o tacto).
- [ ] **Biblioteca de materiales predefinidos** (Oro, Hierro oxidado, Madera, Vidrio semitransparente, Plástico, Agua) incluyendo soporte de **transparencia / alpha**.

---

## 3. Físicas y Simulación (`forja3d-physics`)
- [ ] **Masa automática proporcional al volumen/escala** (opcional por pieza o al escalar).
- [ ] **Uniones y articulaciones (`Joints`)**: bisagras, resortes y uniones rígidas para puertas, ruedas, péndulos y vehículos.
- [ ] **Colisionadores compuestos (`Compound`)** por grupo o pieza, y soporte analítico para **Elipsoide / Cilindro / Cono**.
- [ ] **Generador de colisión convexa automática** (*convex hull* simplificado) para mallas complejas.
- [ ] **BVH / Grilla espacial** para mantener alto rendimiento con decenas de piezas o terrenos densos.
- [ ] **Interacción en vivo (*Physics Grab*)**: agarrar/arrastrar objetos con el ratón durante la simulación + visualización de velocidad/trayectoria.

---

## 4. Esqueleto, Rigging y Animación (`.f3a`)
- [ ] **Editor visual de huesos (`Skeleton` / `Joint`)** en el viewport, aprovechando las estructuras existentes en `forja3d`, con *auto-rig* por proximidad (`VertexSkinning`) y prueba de poses.
- [ ] **Línea de tiempo visual de animación (`.f3a`)** para grabar *keyframes* de posición, rotación y escala y reproducir clips en bucle desde Studio.

---

## 5. Modelado, Escena y Flujo en el Editor
- [ ] **Snapping (imán) entre caras/vértices** y **pivote editable** por pieza.
- [ ] **Jerarquía padre-hijo real** entre piezas (transformaciones en espacio local del padre).
- [ ] **Operaciones de modelado avanzadas**: booleanas y extrusión directa de caras.
- [x] **Vistas rápidas de cámara** (Frontal, Lateral, Superior, Isométrica, Perspectiva / Ortográfica) *(pendiente: soporte de múltiples cámaras)*.

---

## 6. Formatos y Exportación
- [ ] **Exportador a `.glb` (glTF binario)** conservando mallas, texturas, materiales y luces.
- [x] **Archivo de proyecto completo (`.forja` / `.f3s`)** que guarde y cargue escenas enteras con luces de Sol, texturas, estado de texturas por pieza, cámara y configuración física completa.

---

## 7. Modificadores Avanzados, Esculpido y Presentación
- [ ] **Deformadores flexibles (Bending/Twisting/Tapering avanzado)**: elección de eje de deformación (X/Y/Z), ángulo, rango y subdivisión automática en tiempo real.
- [ ] **Jaulas de deformación (*Lattice*)** y **extrusión sobre curva Bézier** para cables, tuberías y arcos orgánicos.
- [ ] **Modificador de Bisel (*Bevel / Chamfer*)** para redondear bordes filosos y dar grosor a superficies.
- [ ] **Esculpido 3D (*Sculpting*) con simetría**: pinceles de inflar, suavizar, aplanar y mover con simetría automática.
- [ ] **Selección proporcional (*Soft Selection*)** al editar vértices vecinos con radio de caída suave.
- [ ] **Render de presentación / Turntable 360°**: captura de imágenes en alta resolución con fondo transparente para iconos y sprites.

