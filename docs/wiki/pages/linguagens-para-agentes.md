# Linguagens para agentes

O estudo original afirma que nenhuma linguagem foi desenhada para LLMs. Em outubro de 2026 isso
já não vale: há várias, todas recentes. Esta página registra o que cada uma decidiu, o estado
de cada uma e o que a família toda tem em comum — e o que nenhuma delas tem: evidência medida
de que o design ajuda o modelo.

## As que se declaram feitas para agentes

| linguagem | o que decidiu para LLMs | estado |
| --- | --- | --- |
| [BAML](https://github.com/BoundaryML/baml) | "the programming language for agents"; parece TypeScript; tipos persistem em runtime, sem `any`; erros tipados e analisados estaticamente; testes e avaliação embutidos; concorrência sem cor com green threads; chamável de TS, Python, Go, C#, Java | pré-1.0, builds noturnos |
| MoonBit | "flattened design": assinaturas obrigatórias no nível do módulo e definições locais separadas, para geração linear; amostrador que reamostra por sintaxe e por tipos com backtracking (ganho de taxa de compilação anunciado sem números absolutos); testes de expectativa | v0.10.x em 2026, 1.0 ainda não publicada |
| [Pel](https://arxiv.org/abs/2505.13453) | gramática mínima, homoicônica, pensada para geração restrita; controle de capacidades na sintaxe; condições em linguagem natural avaliadas por LLM | artigo sem avaliação empírica |
| [Quasar](https://arxiv.org/abs/2506.12202) | separa lógica interna de chamadas de ferramenta com anotação de efeito, para controle de acesso e paralelização | COLM 2026 |
| [NanoLang](https://github.com/jordanhubbard/nanolang) | "designed for machines to write and humans to read"; blocos de teste obrigatórios; spec em JSON para o modelo | sem resultados medidos |
| Zero (Vercel Labs) | começou com diagnósticos JSON de código estável e `zero fix --plan --json`; o README atual virou "graph-native", com o programa como banco de dados semântico editado por `zero query` e `zero patch` | experimental, "expect breaking changes" |

## Precedentes por aspecto

As linguagens que o estudo original lista como referência continuam valendo, cada uma por um
aspecto; o estado delas em 2026 está nas páginas de cada tema.

- **Sintaxe derivada do Python com desempenho nativo:** Mojo (1.0 em 11/08/2026, compilador
  aberto sob Apache 2.0 em 18/08/2026), Codon, Cython — ver [[estrategia-de-transpilacao]].
- **Semântica de valor e contagem de referências:** Swift, Hylo, Lobster, Koka, Lean 4, Roc,
  Nim — ver [[modelo-de-memoria]].
- **Concorrência sem cor:** Go, Java com threads virtuais, Zig, BEAM — ver
  [[concorrencia-sem-cor]].
- **Efeitos:** Koka, Effekt, Unison, Flix — ver [[sistema-de-tipos]].
- **Diagnósticos e compilador como ferramenta:** Rust, Elm, gopls com MCP embutido — ver
  [[compilador-semantico]].
- **Gramáticas para decodificação:** tree-sitter, GBNF, llguidance — ver
  [[decodificacao-restrita]].

## Ensaios de praticantes

- **Armin Ronacher, *A Language For Agents*** (blog, 9/2/2026): "whitespace-based indentation is a problem"; agentes "are afraid of" exceções e
  preferem resultados tipados; defende efeitos explícitos, raciocínio local e nomes
  encontráveis por busca textual.
- **Haupt, *Markov*** (2026): tipos soma com `match` exaustivo e erros do compilador
  "phrased as prompts with suggestions… formatted as diffs".

Opinião, não dado — mas coincidem com a evidência de [[compilador-semantico]] sobre edição e
diagnósticos.

## O que a família tem em comum

Erros tipados ou resultados no lugar de exceções, efeitos ou capacidades explícitos, testes
embutidos e ferramentas legíveis por máquina. A proposta da linguagem X converge com todas.

**Nenhuma publicou evidência controlada de que o design melhora a acurácia dos modelos.** É o
espaço que o [[harness-de-avaliacao]] deste projeto pode ocupar: o diferencial não é ter as
mesmas features, é medir quais delas importam.
