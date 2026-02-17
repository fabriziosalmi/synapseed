
import shutil
import subprocess
import sys
from pathlib import Path

def get_architecture_mermaid() -> str:
    return """graph TD
    User([User / IDE]) <-->|Query/Response| MCP[MCP Server]
    
    subgraph "Synapseed Middleware"
        MCP -->|Router| Tools[Tool Registry]
        MCP -->|Search| Graph[Semantic Graph]
        
        Graph <--> Memory[Dual-Track Memory]
        Memory -.->|Hot| WorkingSet(Working Set)
        Memory -.->|Cold| JourneyMap(Journey Map)
    end
    
    Graph -->|Context Retrieval| LLM[LLM Interface]
    Tools -->|Functions| LLM
    
    LLM <-->|Inference| Models((Base Models\nQwen))
    LLM -.->|Read| Codebase[Codebase Files]
    
    classDef box fill:#fff,stroke:#333,stroke-width:2px;
    classDef core fill:#e1f5fe,stroke:#01579b,stroke-width:2px;
    classDef ext fill:#f3e5f5,stroke:#4a148c,stroke-width:1px;
    
    class MCP,Graph,Memory core;
    class User,Models,Codebase box;
    class Tools,WorkingSet,JourneyMap ext;
"""

def get_sequence_mermaid() -> str:
    return """sequenceDiagram
    participant U as User
    participant S as Synapseed (RAG)
    participant L as LLM
    participant T as Tools (MCP)
    
    U->>S: Query: "Why is auth failing?"
    activate S
    S->>S: Hybrid RRF (Vector + Graph)
    S->>L: Prompt + Initial Context
    activate L
    L->>L: Thought: Need to check auth logs
    L->>T: Call: search_logs("auth error")
    activate T
    T-->>L: Returns: "Error 401 in login.rs"
    deactivate T
    
    L->>L: Thought: Check login.rs logic
    L->>T: Call: read_file("src/auth/login.rs")
    activate T
    T-->>L: Returns: Function verify_token()...
    deactivate T
    
    L->>S: Final Answer Construction
    deactivate L
    S-->>U: "Auth fails due to expired JWT check in login.rs"
    deactivate S
"""

def generate_mermaid_diagrams(output_dir: Path):
    """Generates .mmd files and attempts to render them using mmdc."""
    diagrams = {
        "diagram_arch_mermaid": get_architecture_mermaid(),
        "diagram_seq_mermaid": get_sequence_mermaid()
    }
    
    mmdc_path = shutil.which("mmdc")
    
    for name, content in diagrams.items():
        mmd_file = output_dir / f"{name}.mmd"
        pdf_file = output_dir / f"{name}.pdf"
        
        # Write Source
        with open(mmd_file, "w") as f:
            f.write(content)
            
        # Attempt Render
        if mmdc_path:
            try:
                print(f"   ⚙️ Rendering {name} with mmdc...")
                subprocess.run(
                    [mmdc_path, "-i", str(mmd_file), "-o", str(pdf_file), "-b", "transparent"],
                    check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
                )
                print(f"   ✅ Generated {pdf_file.name}")
            except subprocess.CalledProcessError:
                print(f"   ⚠️ Failed to render {name} with mmdc. Source saved.")
        else:
            print(f"   ⚠️ 'mmdc' not found. Saved Mermaid source to {mmd_file.name}. Please install mermaid-cli for auto-generation.")

if __name__ == "__main__":
    out = Path("assets")
    out.mkdir(exist_ok=True)
    generate_mermaid_diagrams(out)
