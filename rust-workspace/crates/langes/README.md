# Langes

Crate `#![no_std]` + `alloc` y sin dependencias externas para tablas de localización de idiomas (*i18n*) y diccionarios de texto clave-valor en Forja3D.

Permite cargar y consultar tablas de texto (soporta comentarios `#`, claves `clave = valor` y secciones de idioma `[es]`, `[en]`) de forma segura y eficiente tanto en escritorio/móvil como en WebAssembly y microcontroladores sin `std`.
