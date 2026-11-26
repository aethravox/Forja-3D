# Próximamente en Forja-3D

Hoja de ruta de mejoras planificadas para **Forja-3D** (`forja3d`, `forja3d-physics` y `forja3d-studio`).

---

## 1. Gráficos, Iluminación y Viewport
- [ ] **Sombras en tiempo real (Shadow Mapping)** para los objetos **Sol**, con mapa de sombras direccional y suavizado (PCF).
- [x] **Shader PBR real (Cook-Torrance)** con Fresnel y respuesta física realista a `metallic`, `roughness` e intensidad del Sol.
- [ ] **Luces puntuales y focos (`PointLight` / `SpotLight`)** además del Sol (faroles, motores, antorchas, interiores).
- [ ] **Cielo atmosférico / HDRI procedural** ligado a la dirección e intensidad del Sol.
- [ ] **Post-procesado**: niebla atmosférica, **Bloom** (brillo en luces y emisivos) y **Anti-aliasing (MSAA)**.
- [ ] **Oclusión ambiental en espacio de pantalla (SSAO)** para dar profundidad realista en esquinas y pliegues.
- [ ] **Modos *MatCap*** (arcilla, metal pulido, cebra de curvatura) para esculpir y revisar topología sin luces complejas.
- [ ] **Soporte de *Skybox* / mapa cúbico personalizado** además del cielo procedural.
- [x] **Overlays de diagnóstico**: visualizador de normales, cuadrícula *UV Checker* y mapa de densidad de triángulos.
- [ ] **Shader *Toon / Cel-Shading*** con contorno (*Outline*) activable por material o escena.
- [ ] **Sistema de partículas ligero en Studio** (chispas, humo, fuego, estelas) exportable para juegos.
- [ ] **Vista cuádruple (*Quad-View*)** simultánea (Perspectiva + Frontal + Lateral + Superior).

---

## 2. Materiales, UVs, Texturas (`.f3t`) y Pintado 3D
- [ ] **Mapas de Normal, Roughness y Emissive** además de la textura base (*albedo*).
- [x] **Tiling y Offset de UVs por pieza** (repetición, escala y desplazamiento para evitar estiramientos en modelos grandes).
- [ ] **Pincel de pintado 3D en el viewport** (pintar vértices o textura directamente con ratón o tacto).
- [x] **Biblioteca de materiales predefinidos** (Oro, Hierro oxidado, Madera, Vidrio semitransparente, Plástico, Agua) incluyendo soporte de **transparencia ordenada (*Alpha Sorting* y *Alpha Cutout*)**.
- [ ] **Editor visual de UVs 2D** con proyección automática (*Box*, *Planar*, *Cylindrical*, *Smart Unwrap*).
- [ ] **Horneado (*Baking*) de mapas** desde geometría o luces: *Ambient Occlusion*, *Curvature*, *Normal Map* y *Vertex Color -> `.f3t`*.
- [ ] **Generador procedural de texturas dentro de Studio** (ruido, ladrillos, madera, metal cepillado, cuadrícula) guardado directo en `.f3t`.
- [ ] **Gestor de paletas de color** (importar/exportar `.hex`/`.gpl` y recoloración rápida por grupo o rampa).
- [ ] **Capas de pintado con opacidad y desgaste por bordes** antes de hornear a `.f3t`.
- [ ] **Generación de mipmaps y previsualización de compresión en `.f3t`** con vista previa de peso en KB y calidad.
- [ ] **Materiales animados por shader** (desplazamiento UV de agua/lava, pulso emisivo, efecto holograma).

---

## 3. Físicas y Simulación (`forja3d-physics`)
- [ ] **Masa automática proporcional al volumen/escala** (opcional por pieza o al escalar).
- [ ] **Uniones y articulaciones (`Joints`)**: bisagras, resortes y uniones rígidas para puertas, ruedas, péndulos y vehículos.
- [ ] **Colisionadores compuestos (`Compound`)** por grupo o pieza, y soporte analítico para **Elipsoide / Cilindro / Cono**.
- [ ] **Generador de colisión convexa automática** (*convex hull* simplificado) para mallas complejas.
- [ ] **BVH / Grilla espacial** para mantener alto rendimiento con decenas de piezas o terrenos densos.
- [ ] **Interacción en vivo (*Physics Grab*)**: agarrar/arrastrar objetos con el ratón durante la simulación + visualización de velocidad/trayectoria.
- [ ] **Detección continua de colisiones (CCD)** para evitar que objetos rápidos atraviesen paredes o suelos delgados.
- [ ] **Reposo automático de cuerpos (*Body Sleeping*)** y activación por impacto para optimizar escenas densas.
- [ ] **Controlador de personaje (*Character Controller*)** nativo (subida de escalones, límite de pendiente y detección de suelo).
- [ ] **Física de vehículo (*Raycast Vehicle*)** con suspensión, dirección y 4 ruedas probables en tiempo real.
- [ ] **Volúmenes de fuerza y fluidos**: viento direccional, gravedad local, flotabilidad en agua y explosiones radiales.
- [ ] **Capas y máscaras de colisión (`CollisionLayers`)** configurables por pieza o grupo.
- [ ] **Horneado de simulación física a animación `.f3a`** para grabar caídas/choques como keyframes reproducibles.
- [ ] **Fractura simple / cuerpos destructibles** para dividir piezas en fragmentos físicos ante impactos fuertes.

---

## 4. Esqueleto, Rigging y Animación (`.f3a`)
- [ ] **Editor visual de huesos (`Skeleton` / `Joint`)** en el viewport, aprovechando las estructuras existentes en `forja3d`, con *auto-rig* por proximidad (`VertexSkinning`) y prueba de poses.
- [ ] **Línea de tiempo visual de animación (`.f3a`)** para grabar *keyframes* de posición, rotación y escala y reproducir clips en bucle desde Studio.
- [ ] **Cinemática inversa (IK de 2 huesos y FABRIK)** para posar extremidades arrastrando solo el extremo.
- [ ] **Morfos / *Blend Shapes*** para expresiones faciales o estados deformables animables en `.f3a`.
- [ ] **Editor de curvas (*Graph Editor*)** con tangentes Bézier (*Ease In/Out*, rebote, elástico).
- [ ] **Máquina de estados y mezcla (*Blending*) de clips** en `.f3a` (`Idle -> Walk -> Jump`).
- [ ] **Pincel visual de pesos de huesos (*Weight Painting*)** con auto-suavizado por calor sobre vértices.

---

## 5. Modelado, Escena y Flujo en el Editor
- [ ] **Snapping (imán) entre caras/vértices** y **pivote editable** por pieza.
- [ ] **Jerarquía padre-hijo real** entre piezas (transformaciones en espacio local del padre).
- [ ] **Operaciones de modelado avanzadas**: booleanas, extrusión directa de caras, *Inset*, *Bridge* y corte libre (*Knife*).
- [ ] **Edición de aristas y bucles (*Edge/Face Loop & Ring*)** con subdivisión localizada.
- [ ] **Selección múltiple (`Shift` + clic y *Box Select*)** para transformar o agrupar varias piezas a la vez.
- [ ] **Duplicado instanciado (*Linked Duplicate*)** para compartir geometría base entre varias piezas.
- [x] **Alineación y apoyo automático** (centros/bordes en X/Y/Z y sentar piezas sobre el suelo o terreno).
- [ ] **Regla / medidor 3D en vivo** de distancias, ángulos y dimensiones sobre el viewport.
- [x] **Buscador y filtro rápido en el árbol de escena** + modo **Aislar (*Solo*)** y **Bloqueo de piezas (*Lock*)**.
- [ ] **Paleta de comandos rápida (`Ctrl+K` / `Espacio`)** para ejecutar herramientas o primitivas al instante.
- [x] **Vistas rápidas de cámara** (Frontal, Lateral, Superior, Isométrica, Perspectiva / Ortográfica) *(pendiente: soporte de múltiples cámaras)*.

---

## 6. Formatos, Codegen y Exportación
- [x] **Exportador a `.glb` (glTF binario)** conservando mallas, texturas, materiales y luces.
- [x] **Archivo de proyecto completo (`.forja` / `.f3s`)** que guarde y cargue escenas enteras con luces de Sol, texturas, estado de texturas por pieza, cámara y configuración física completa.
- [x] **Exportador `.stl` / `.ply` (impresión 3D) + validador *watertight*** que detecta agujeros, bordes sueltos y normales invertidas.
- [ ] **Plantillas de exportación `.rs` para motores Rust** (`bevy`, `macroquad`, `three-d`, `wgpu`) incluyendo luces, físicas y `.f3a`.
- [ ] **Generador automático de LODs (`LOD0` / `LOD1` / `LOD2`)** mediante simplificación de triángulos al exportar `.f3d`/`.f3p`.

---

## 7. Modificadores, Terrenos, Esculpido y Presentación
- [ ] **Deformadores flexibles (Bending/Twisting/Tapering avanzado)**: elección de eje de deformación (X/Y/Z), ángulo, rango y subdivisión automática en tiempo real.
- [ ] **Jaulas de deformación (*Lattice*)** y **extrusión sobre curva Bézier** para cables, tuberías y arcos orgánicos.
- [ ] **Modificador de Bisel (*Bevel / Chamfer*)** para redondear bordes filosos y dar grosor a superficies.
- [ ] **Esculpido 3D (*Sculpting*) con simetría**: pinceles de inflar, suavizar, aplanar y mover con simetría automática.
- [ ] **Selección proporcional (*Soft Selection*)** al editar vértices vecinos con radio de caída suave.
- [ ] **Nuevos pinceles de terreno**: *Smooth* (suavizar), *Flatten* (aplanar a cota), *Ramp* (rampa entre 2 puntos) y erosión básica.
- [ ] **Pincel de dispersión (*Scatter / Foliage*)** para plantar rocas, árboles o props sobre el terreno con variación aleatoria.
- [ ] **Ensamblado modular por grilla (*Grid Snapping* + conectores)** y **generador procedural de árboles/vegetación (*L-System*)**.
- [ ] **Render de presentación / Turntable 360°**: captura de imágenes en alta resolución con fondo transparente para iconos y sprites.

---

## 8. Interfaz 2D (`.f2d`) y Productividad del Estudio
- [ ] **Transiciones y estados (*Hover/Pressed/Animación*) en `.f2d`** con fuentes bitmap/SDF configurables.
- [ ] **Previsualización del HUD `.f2d` superpuesto al viewport 3D** para probar menús encima de la escena real.
- [ ] **Historial visual de *Undo/Redo* + autoguardado de recuperación** ante cierres inesperados.
- [ ] **Biblioteca local de *Prefabs*** para guardar piezas o grupos complejos y reutilizarlos entre proyectos `.forja`.
- [ ] **Perfilador de presupuesto en vivo** (draw calls estimados, VRAM de texturas, peso estándar vs compacto `.f3d` y presupuesto móvil/WASM).
