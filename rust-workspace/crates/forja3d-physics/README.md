# forja3d-physics

Motor de físicas 3D `#![no_std]` y sin dependencias externas diseñado de forma nativa para modelos **`.f3d`** de **Forja3D**.

Cuando un modelo `.f3d` incluye `FLAG_PHYSICS` (`PhysicsSpec` + colisionadores por pieza), basta con cargarlo e instanciarlo con `PhysicsWorld::spawn_from_model(&model, pos)` para que el modelo tenga colisiones, masa, fricción, rebote, sensores y raycasts al instante.
