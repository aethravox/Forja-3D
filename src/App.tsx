import { useEffect, useRef, useState } from 'react';
import {
  Box,
  Check,
  ChevronDown,
  Copy,
  Download,
  FileCode2,
  FolderTree,
  Layers,
  RefreshCw,
} from 'lucide-react';
import initWasm, { start_forja3d_canvas } from './wasm/forja3d_studio.js';
import wasmUrl from './wasm/forja3d_studio_bg.wasm?url';

type ActiveTab = 'studio' | 'workspace' | 'format';

interface WorkspaceFile {
  path: string;
  content: string;
  bytes: number;
}

export default function App() {
  const [activeTab, setActiveTab] = useState<ActiveTab>('studio');
  const [wasmState, setWasmState] = useState<'loading' | 'ready' | 'error' | 'context-lost'>('loading');
  const [wasmError, setWasmError] = useState<string>('');
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [filesLoading, setFilesLoading] = useState<boolean>(true);
  const [selectedPath, setSelectedPath] = useState<string>('crates/forja3d/src/format.rs');
  const [mobileFilesOpen, setMobileFilesOpen] = useState<boolean>(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const wasmStartedRef = useRef(false);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  const bootWasm = async () => {
    if (wasmStartedRef.current) return;
    wasmStartedRef.current = true;
    setWasmState('loading');
    setWasmError('');
    try {
      await initWasm({ module_or_path: wasmUrl });
      start_forja3d_canvas('forja3d-canvas');
      setWasmState('ready');
    } catch (err) {
      wasmStartedRef.current = false;
      setWasmState('error');
      setWasmError(err instanceof Error ? err.message : 'No se pudo inicializar WebAssembly/WebGL2');
    }
  };

  useEffect(() => {
    bootWasm();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const handleLost = (e: Event) => {
      e.preventDefault();
      setWasmState('context-lost');
    };
    const handleRestored = () => {
      setWasmState('ready');
    };

    canvas.addEventListener('webglcontextlost', handleLost);
    canvas.addEventListener('webglcontextrestored', handleRestored);
    return () => {
      canvas.removeEventListener('webglcontextlost', handleLost);
      canvas.removeEventListener('webglcontextrestored', handleRestored);
    };
  }, []);

  useEffect(() => {
    let active = true;
    fetch('/api/workspace-files')
      .then((r) => r.json())
      .then((data: { files?: WorkspaceFile[] }) => {
        if (!active) return;
        const list = data.files ?? [];
        setFiles(list);
        if (list.length > 0 && !list.some((f) => f.path === selectedPath)) {
          setSelectedPath(list[0].path);
        }
        setFilesLoading(false);
      })
      .catch(() => {
        if (active) setFilesLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const activeFile = files.find((f) => f.path === selectedPath);

  const copyText = async (id: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
      setTimeout(() => setCopiedId((prev) => (prev === id ? null : prev)), 1800);
    } catch {
      // Ignore clipboard failures in restricted iframes
    }
  };

  return (
    <div className="h-[100dvh] w-screen flex flex-col overflow-hidden bg-[#0B0D11] text-[#E6E9EF]">
      {/* Desktop Top Bar (md+): strict 3-zone Top Bar Contract */}
      <header className="h-11 md:h-14 shrink-0 flex items-center justify-between gap-4 md:gap-8 px-4 md:px-6 border-b border-white/10 bg-[#0F1218]">
        <a
          href="#studio"
          onClick={(e) => {
            e.preventDefault();
            setActiveTab('studio');
          }}
          className="font-display text-base md:text-lg font-semibold tracking-tight text-white whitespace-nowrap shrink-0"
        >
          Forja3D
        </a>

        <nav className="hidden md:flex items-center gap-6 text-sm font-medium">
          <button
            type="button"
            onClick={() => setActiveTab('studio')}
            className={`py-1 transition-colors whitespace-nowrap shrink-0 border-b-2 ${
              activeTab === 'studio'
                ? 'border-amber-500 text-white'
                : 'border-transparent text-slate-400 hover:text-slate-100'
            }`}
          >
            Studio WASM
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('workspace')}
            className={`py-1 transition-colors whitespace-nowrap shrink-0 border-b-2 ${
              activeTab === 'workspace'
                ? 'border-amber-500 text-white'
                : 'border-transparent text-slate-400 hover:text-slate-100'
            }`}
          >
            Crates.io Workspace
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('format')}
            className={`py-1 transition-colors whitespace-nowrap shrink-0 border-b-2 ${
              activeTab === 'format'
                ? 'border-amber-500 text-white'
                : 'border-transparent text-slate-400 hover:text-slate-100'
            }`}
          >
            Formato .f3d
          </button>
        </nav>

        <div className="flex items-center shrink-0">
          <a
            href="/api/workspace.zip"
            download="forja3d-rust-workspace.zip"
            className="inline-flex items-center gap-1.5 md:gap-2 px-3 md:px-4 py-1.5 md:py-2 text-xs font-semibold text-slate-950 bg-amber-500 hover:bg-amber-400 rounded-md transition-colors whitespace-nowrap shrink-0 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-400"
          >
            <Download className="w-3.5 h-3.5" />
            <span className="md:hidden">Workspace .zip</span>
            <span className="hidden md:inline">Descargar Workspace</span>
          </a>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 min-h-0 relative">
        {/* Live Rust + WebGL2 WASM Canvas stays mounted with non-zero dimensions */}
        <div
          className={`absolute inset-0 transition-opacity duration-150 ${
            activeTab === 'studio' ? 'opacity-100 pointer-events-auto z-10' : 'opacity-0 pointer-events-none z-0'
          }`}
        >
          <canvas
            ref={canvasRef}
            id="forja3d-canvas"
            className="w-full h-full block bg-[#0B0D11] outline-none touch-none select-none"
          />

          {wasmState === 'loading' && (
            <div className="absolute inset-0 flex flex-col items-center justify-center bg-[#0B0D11]/95 z-20 px-6 text-center">
              <div className="w-8 h-8 border-2 border-amber-500/30 border-t-amber-500 rounded-full animate-spin mb-4" />
              <p className="font-display text-base font-medium text-white">
                Cargando motor Rust en WebAssembly
              </p>
              <p className="text-xs text-slate-400 mt-1">
                Iniciando interfaz egui y renderizador WebGL2
              </p>
            </div>
          )}

          {(wasmState === 'error' || wasmState === 'context-lost') && (
            <div className="absolute inset-0 flex flex-col items-center justify-center bg-[#0B0D11]/95 z-20 px-6 text-center">
              <p className="font-display text-lg font-semibold text-white">
                {wasmState === 'context-lost'
                  ? 'Contexto WebGL2 interrumpido'
                  : 'No se pudo iniciar WebAssembly'}
              </p>
              <p className="text-sm text-slate-400 max-w-md mt-2">
                {wasmError || 'El navegador suspendió el contexto gráfico. Puedes reintentar o descargar el código fuente para compilarlo nativo.'}
              </p>
              <div className="flex items-center gap-3 mt-5">
                <button
                  type="button"
                  onClick={() => {
                    wasmStartedRef.current = false;
                    bootWasm();
                  }}
                  className="inline-flex items-center gap-2 min-h-[44px] px-4 py-2 text-xs font-semibold text-slate-950 bg-amber-500 hover:bg-amber-400 rounded-md transition-colors"
                >
                  <RefreshCw className="w-3.5 h-3.5" />
                  Reintentar
                </button>
                <button
                  type="button"
                  onClick={() => setActiveTab('workspace')}
                  className="min-h-[44px] px-4 py-2 text-xs font-medium text-slate-200 border border-white/15 hover:bg-white/5 rounded-md transition-colors"
                >
                  Ver código Rust
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Crates.io Workspace Explorer */}
        {activeTab === 'workspace' && (
          <section className="absolute inset-0 z-20 bg-[#0B0D11] flex flex-col md:flex-row overflow-hidden">
            {/* Mobile File Switcher Bar */}
            <div className="md:hidden shrink-0 border-b border-white/10 bg-[#0F1218]">
              <button
                type="button"
                onClick={() => setMobileFilesOpen((v) => !v)}
                className="w-full min-h-[44px] px-4 py-2 flex items-center justify-between gap-3 text-left"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <FolderTree className="w-4 h-4 text-amber-400 shrink-0" />
                  <span className="font-mono text-xs text-slate-100 truncate">
                    {activeFile ? activeFile.path : 'Seleccionar archivo'}
                  </span>
                </div>
                <ChevronDown
                  className={`w-4 h-4 text-slate-400 shrink-0 transition-transform ${
                    mobileFilesOpen ? 'rotate-180' : ''
                  }`}
                />
              </button>

              {mobileFilesOpen && (
                <div className="max-h-60 overflow-y-auto border-t border-white/10 p-2 space-y-1 bg-[#0B0D11]">
                  {files.map((file) => {
                    const isSelected = file.path === selectedPath;
                    return (
                      <button
                        key={file.path}
                        type="button"
                        onClick={() => {
                          setSelectedPath(file.path);
                          setMobileFilesOpen(false);
                        }}
                        className={`w-full min-h-[44px] text-left px-3 py-2 rounded-md text-xs font-mono flex items-center justify-between gap-2 ${
                          isSelected
                            ? 'bg-amber-500/15 text-amber-300'
                            : 'text-slate-300 hover:bg-white/5'
                        }`}
                      >
                        <span className="truncate">{file.path}</span>
                        <span className="text-[11px] text-slate-500 shrink-0 tabular-nums">
                          {(file.bytes / 1024).toFixed(1)} KB
                        </span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Desktop File Tree Sidebar */}
            <aside className="hidden md:flex w-80 shrink-0 border-r border-white/10 bg-[#0F1218] flex-col min-h-0">
              <div className="p-4 border-b border-white/10">
                <div className="flex items-center gap-2 text-sm font-semibold text-white">
                  <FolderTree className="w-4 h-4 text-amber-400" />
                  <span>Workspace Rust</span>
                </div>
                <p className="text-xs text-slate-400 mt-1">
                  2 crates independientes listos para publicar en crates.io
                </p>
              </div>

              <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
                {filesLoading ? (
                  <p className="text-xs text-slate-400 p-3">Cargando archivos...</p>
                ) : (
                  files.map((file) => {
                    const isSelected = file.path === selectedPath;
                    return (
                      <button
                        key={file.path}
                        type="button"
                        onClick={() => setSelectedPath(file.path)}
                        className={`w-full text-left px-3 py-2 rounded-md text-xs font-mono flex items-center justify-between gap-2 transition-colors ${
                          isSelected
                            ? 'bg-amber-500/15 text-amber-300'
                            : 'text-slate-300 hover:bg-white/5 hover:text-white'
                        }`}
                      >
                        <span className="truncate">{file.path}</span>
                        <span className="text-[11px] text-slate-500 shrink-0 tabular-nums">
                          {(file.bytes / 1024).toFixed(1)} KB
                        </span>
                      </button>
                    );
                  })
                )}
              </div>

              <div className="p-4 border-t border-white/10 space-y-2 text-xs text-slate-400">
                <p className="font-medium text-slate-200">Comandos multiplataforma</p>
                <div className="font-mono text-[11px] bg-[#090B0E] p-2.5 rounded border border-white/10 space-y-1 text-slate-300 overflow-x-auto">
                  <div>cargo test -p forja3d</div>
                  <div>cargo run -p forja3d-studio</div>
                  <div>cargo apk build -p forja3d-studio</div>
                  <div>cargo publish -p forja3d</div>
                </div>
              </div>
            </aside>

            <div className="flex-1 min-w-0 min-h-0 flex flex-col bg-[#0B0D11]">
              {activeFile ? (
                <>
                  <div className="h-12 shrink-0 px-4 md:px-5 border-b border-white/10 flex items-center justify-between gap-4 bg-[#0D1016]">
                    <div className="flex items-center gap-2 min-w-0">
                      <FileCode2 className="w-4 h-4 text-slate-400 shrink-0" />
                      <span className="font-mono text-xs text-slate-200 truncate">
                        {activeFile.path}
                      </span>
                      <span className="hidden sm:inline text-xs text-slate-500 shrink-0">·</span>
                      <span className="hidden sm:inline text-xs text-slate-400 tabular-nums shrink-0">
                        {activeFile.bytes} bytes
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => copyText('file', activeFile.content)}
                      className="inline-flex items-center gap-1.5 min-h-[36px] px-3 py-1.5 text-xs font-medium text-slate-200 border border-white/15 hover:bg-white/5 rounded transition-colors shrink-0"
                    >
                      {copiedId === 'file' ? (
                        <>
                          <Check className="w-3.5 h-3.5 text-amber-400" />
                          Copiado
                        </>
                      ) : (
                        <>
                          <Copy className="w-3.5 h-3.5" />
                          Copiar
                        </>
                      )}
                    </button>
                  </div>
                  <div className="flex-1 overflow-auto p-4 md:p-5">
                    <pre className="font-mono text-[11px] md:text-xs leading-6 text-slate-200 whitespace-pre">
                      {activeFile.content}
                    </pre>
                  </div>
                </>
              ) : (
                <div className="flex-1 flex items-center justify-center text-sm text-slate-400 p-6 text-center">
                  Selecciona un archivo para ver su código fuente.
                </div>
              )}
            </div>
          </section>
        )}

        {/* .f3d Binary Format Specification */}
        {activeTab === 'format' && (
          <section className="absolute inset-0 z-20 bg-[#0B0D11] overflow-y-auto">
            <div className="max-w-5xl mx-auto px-4 py-6 md:px-6 md:py-10 space-y-6 md:space-y-10">
              <div className="space-y-3 max-w-2xl">
                <p className="text-xs font-medium text-amber-400">
                  Especificación binaria F3D1 · Zero dependencies · no_std + alloc
                </p>
                <h1 className="font-display text-2xl md:text-3xl font-semibold text-white text-balance">
                  Formato binario .f3d y exportación directa a código .rs
                </h1>
                <p className="text-sm leading-relaxed text-slate-300">
                  El crate <code className="text-amber-300">forja3d</code> permite distribuir modelos 3D de dos maneras:
                  como archivo binario compacto <code className="text-slate-100">.f3d</code> con verificación de integridad FNV-1a,
                  o compilado directamente como módulo <code className="text-slate-100">.rs</code> sin parsers ni dependencias externas.
                </p>
              </div>

              <div className="grid grid-cols-1 lg:grid-cols-2 gap-5 md:gap-6 items-stretch">
                <div className="bg-[#11141B] border border-white/10 rounded-lg p-4 md:p-6 space-y-4">
                  <h2 className="font-display text-base font-semibold text-white">
                    Cabecera binaria (16 bytes, Little-Endian)
                  </h2>
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs font-mono">
                      <thead>
                        <tr className="border-b border-white/10 text-slate-400">
                          <th className="py-2 pr-4 font-medium">Offset</th>
                          <th className="py-2 pr-4 font-medium">Tipo</th>
                          <th className="py-2 font-medium">Campo</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-white/5 text-slate-200 tabular-nums">
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">0x00..0x03</td>
                          <td className="py-2 pr-4 whitespace-nowrap">[u8; 4]</td>
                          <td className="py-2 font-sans">Magic bytes &quot;F3D1&quot;</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">0x04..0x05</td>
                          <td className="py-2 pr-4 whitespace-nowrap">u16</td>
                          <td className="py-2 font-sans">Versión del formato (1)</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">0x06..0x07</td>
                          <td className="py-2 pr-4 whitespace-nowrap">u16</td>
                          <td className="py-2 font-sans">Flags (0 = estándar 36 B/v, 1 = compacto 17 B/v + u16 idx)</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">0x08..0x0B</td>
                          <td className="py-2 pr-4 whitespace-nowrap">u32</td>
                          <td className="py-2 font-sans">Cantidad de sub-mallas (MeshPart)</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">0x0C..0x0F</td>
                          <td className="py-2 pr-4 whitespace-nowrap">u32</td>
                          <td className="py-2 font-sans">Checksum FNV-1a 32-bit del payload</td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>

                <div className="bg-[#11141B] border border-white/10 rounded-lg p-4 md:p-6 space-y-4">
                  <h2 className="font-display text-base font-semibold text-white">
                    Estructura de Vértice (36 bytes por vértice)
                  </h2>
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs font-mono">
                      <thead>
                        <tr className="border-b border-white/10 text-slate-400">
                          <th className="py-2 pr-4 font-medium">Tamaño</th>
                          <th className="py-2 pr-4 font-medium">Tipo</th>
                          <th className="py-2 font-medium">Atributo</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-white/5 text-slate-200 tabular-nums">
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">12 B</td>
                          <td className="py-2 pr-4 whitespace-nowrap">[f32; 3]</td>
                          <td className="py-2 font-sans">Posición XYZ en espacio modelo</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">12 B</td>
                          <td className="py-2 pr-4 whitespace-nowrap">[f32; 3]</td>
                          <td className="py-2 font-sans">Normal normalizada XYZ</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">4 B</td>
                          <td className="py-2 pr-4 whitespace-nowrap">[u8; 4]</td>
                          <td className="py-2 font-sans">Color sRGBA por vértice</td>
                        </tr>
                        <tr>
                          <td className="py-2 pr-4 text-amber-300 whitespace-nowrap">8 B</td>
                          <td className="py-2 pr-4 whitespace-nowrap">[f32; 2]</td>
                          <td className="py-2 font-sans">Coordenadas UV</td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              </div>

              <div className="bg-[#11141B] border border-white/10 rounded-lg p-4 md:p-6 space-y-4">
                <div className="flex items-center justify-between gap-4">
                  <h2 className="font-display text-base font-semibold text-white">
                    Uso del crate cargador (crates.io)
                  </h2>
                  <button
                    type="button"
                    onClick={() =>
                      copyText(
                        'usage',
                        `use forja3d::Model;\n\nstatic MODELO: &[u8] = include_bytes!("../assets/drone.f3d");\n\npub fn cargar_modelo() -> Result<Model, forja3d::DecodeError> {\n    Model::from_bytes(MODELO)\n}`
                      )
                    }
                    className="inline-flex items-center gap-1.5 min-h-[36px] px-3 py-1.5 text-xs font-medium text-slate-200 border border-white/15 hover:bg-white/5 rounded transition-colors shrink-0"
                  >
                    {copiedId === 'usage' ? (
                      <>
                        <Check className="w-3.5 h-3.5 text-amber-400" />
                        Copiado
                      </>
                    ) : (
                      <>
                        <Copy className="w-3.5 h-3.5" />
                        Copiar
                      </>
                    )}
                  </button>
                </div>
                <pre className="font-mono text-xs leading-6 text-slate-200 bg-[#090B0E] p-4 rounded border border-white/10 overflow-x-auto">{`// Cargo.toml: forja3d = "0.1"
use forja3d::Model;

static MODELO: &[u8] = include_bytes!("../assets/drone.f3d");

pub fn cargar_modelo() -> Result<Model, forja3d::DecodeError> {
    let model = Model::from_bytes(MODELO)?;
    println!("Cargado {} ({} triángulos)", model.name, model.triangle_count());
    Ok(model)
}`}</pre>
              </div>
            </div>
          </section>
        )}
      </main>

      {/* Mobile Bottom Thumb Navigation Bar */}
      <nav
        aria-label="Navegación principal móvil"
        className="md:hidden h-12 shrink-0 grid grid-cols-3 border-t border-white/10 bg-[#0F1218]"
      >
        <button
          type="button"
          onClick={() => setActiveTab('studio')}
          className={`min-h-[44px] flex flex-col items-center justify-center gap-0.5 text-[11px] font-medium transition-colors ${
            activeTab === 'studio' ? 'text-amber-400' : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Box className="w-4 h-4" />
          <span>Studio</span>
        </button>
        <button
          type="button"
          onClick={() => setActiveTab('workspace')}
          className={`min-h-[44px] flex flex-col items-center justify-center gap-0.5 text-[11px] font-medium transition-colors ${
            activeTab === 'workspace' ? 'text-amber-400' : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <FolderTree className="w-4 h-4" />
          <span>Workspace</span>
        </button>
        <button
          type="button"
          onClick={() => setActiveTab('format')}
          className={`min-h-[44px] flex flex-col items-center justify-center gap-0.5 text-[11px] font-medium transition-colors ${
            activeTab === 'format' ? 'text-amber-400' : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          <Layers className="w-4 h-4" />
          <span>Formato .f3d</span>
        </button>
      </nav>
    </div>
  );
}
