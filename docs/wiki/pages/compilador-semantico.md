# Compilador semântico

O estudo original chama o compilador de "a metade mais importante do projeto": uma ferramenta
que um agente chama em loop, não só um gerador de binários. A evidência reunida aqui confirma
a tese e corrige três pontos — o formato da confiança das correções, onde os erros realmente
estão e o que o digest consegue — além de acrescentar o que o piloto revelou sobre semântica.

## Onde os erros estão

- **Tipos, não sintaxe.** Em TypeScript gerado por LLMs, "on average 94% of compilation
  errors result from failing type checks"; sintaxe é cerca de 6%
  ([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025).
- **Nomes não resolvidos lideram em Rust.** Na tradução para Rust
  ([RustRepoTrans](https://arxiv.org/abs/2411.13990)), erros de compilação são 94,8% das
  falhas, e os códigos mais frequentes são nome não resolvido (E0425), método inexistente
  (E0599), import não resolvido (E0432) e trait faltando (E0277); tipo incompatível (E0308)
  vem bem atrás.
- **Ownership é minoria.** Em 86.726 erros de sete modelos sobre o CodeNet, ownership e
  lifetimes foram 16,7% ([Nogueira, Vieira e Campos](https://arxiv.org/abs/2608.00661), ISSRE
  2026).
- **Na linguagem X, o piloto achou erros semânticos que o parser aceita** — locais imutáveis
  reatribuídos, listas mutadas sem `var`, argumentos tratados como referência
  ([[piloto-de-vazamento-de-python]]).

Consequência: a prioridade é diagnóstico de nomes e tipos com sugestão ("você quis dizer",
import sugerido) e diagnóstico de mutabilidade com a correção pronta. A gramática resolve a
menor parte do problema.

## Diagnósticos: seguir o modelo do rustc

O formato JSON do estudo original está na direção certa. Os precedentes dizem como fechá-lo:

- **rustc** ([doc](https://doc.rust-lang.org/rustc/json.html)): código com explicação, nível,
  spans com bytes e linha/coluna, `children`, `suggested_replacement` e a política "new fields
  may be added; enumerated fields may add new values".
- **Aplicabilidade discreta em vez de confiança numérica.** O rustc classifica cada sugestão
  como `MachineApplicable` (aplicar automaticamente), `MaybeIncorrect`, `HasPlaceholders` ou
  `Unspecified`, e o `cargo fix` só aplica a primeira por padrão. O `"confidence": 0.95` do
  estudo original deve virar esse enum: um número convida a escolher limiar, um nível diz o
  que fazer.
- **Códigos estáveis nunca reutilizados**, cada um com página de explicação (`rustc
  --explain`); um código aposentado mantém a página marcada como não emitida.
- **Anti-modelos:** o `tsc` não tem saída JSON de diagnósticos; o GCC removeu o formato `json`
  no GCC 16 em favor de SARIF. Recomendação: JSON próprio estável, com exportação SARIF.

## O loop de correção satura em duas ou três rodadas

- "Successful debugging processes mostly end within 3 turns"
  ([Self-Debugging](https://arxiv.org/abs/2304.05128), ICLR 2024), e feedback mais rico
  rendeu mais (MBPP com Codex: 61,4 de base, 68,2 com feedback simples, 70,8 com trace).
- "Most models lose 60–80% of their debugging capability within just 2–3 attempts"
  ([Debugging Decay Index](https://arxiv.org/abs/2506.18403), 2025); recomeçar do zero
  recupera.
- Contado o custo, o ganho do autorreparo é "often modest"
  ([Olausson et al.](https://arxiv.org/abs/2306.09896), ICLR 2024).

Daí três requisitos: **reportar todos os erros de uma vez** (parser tolerante), **causa raiz
primeiro com cascatas suprimidas**, e **aplicar as correções `MachineApplicable` sem gastar uma
rodada do modelo** (`check --fix`).

## O formato da correção importa tanto quanto o diagnóstico

- No [RustAssistant](https://arxiv.org/abs/2308.05177) (ICSE 2025), pedir o trecho revisado
  inteiro derrubou a acurácia para "below 10%"; um formato de changelog com linhas originais e
  corrigidas levou a cerca de 74% nos casos do Stack Overflow. O GPT-4 corrigiu 92,59% de 270
  microbenchmarks, um por código de erro.
- No aider, desligar o patch flexível multiplicou por 9 os erros de edição.

Correções saem como patches estruturados ancorados em símbolos, não em números de linha.

## Diagnóstico detalhado ajuda, com ressalvas

- [Krishnamurthi e Flatt](https://arxiv.org/abs/2606.01522) (2026), 2.400 tentativas com um
  agente: reparo de 24,6–41,2% só com testes, 34,6–47,8% com o erro de tipo mínimo, 40,7–63,4%
  com a pilha completa de unificação; 97,9% dos reparos que passaram no tipo passaram também
  nos testes. Eles propõem modos separados, "terse human" e "detailed AI".
- Feedback detalhado nem sempre ajuda (Zheng et al., ICLR 2025, [arXiv
  2410.08105](https://arxiv.org/abs/2410.08105)) e o ranking dos tipos de feedback muda por
  linguagem ([arXiv 2609.00362](https://arxiv.org/abs/2609.00362)).
- **Não há estudo controlado comparando, para LLMs, diagnósticos ricos no estilo Rust ou Elm
  com diagnósticos secos.** É uma pergunta para o [[harness-de-avaliacao]].

## Verificar a cada edição

No [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024), tirar o lint na edição
derrubou o resultado de 18,0% para 15,0% no SWE-bench Lite. 51,7% das trajetórias tiveram ao
menos uma edição falha, e a chance de sucesso caiu de 90,5% para 57,2% depois da primeira. O
lint desfazia a edição ruim e mostrava o tipo de erro, a edição tentada e o texto original;
tirar qualquer uma das três partes piorou. Por isso a checagem incremental em menos de 100 ms,
meta do estudo original, não é luxo: é o que permite checar toda edição.

## Digest: índice, não substituto

- **Localização:** no [Agentless](https://arxiv.org/abs/2407.01489), um esqueleto de
  assinaturas localizou 58,33% dos problemas contra 53,67% com arquivos inteiros, a US$ 0,02
  contra US$ 0,15.
- **Tipos no contexto:** em [Blinn et al.](https://arxiv.org/abs/2409.00921) (OOPSLA 2024),
  definições de tipo foram "absolutely necessary" e cabeçalhos de função multiplicaram o
  resultado por cerca de 3.
- **Geração:** no [RepoExec](https://arxiv.org/abs/2406.11927) (NAACL 2025 Findings), o
  contexto completo venceu; só assinatura venceu assinatura com docstring.

O digest ajuda a achar e barateia o contexto, mas quem vai alterar uma função ainda precisa
dos corpos das dependências. Desenho: o digest é o índice do projeto e os corpos vêm sob
demanda, por símbolo.

## LSP e MCP

- **Virou padrão:** gopls v0.20 com MCP embutido, `dart mcp-server`, IntelliJ 2025.2, Xcode
  26.3 (`xcrun mcpbridge`), e o Claude Code devolve diagnósticos de LSP após cada edição.
- **O ganho não está provado.** Um estudo preliminar
  ([arXiv 2608.13568](https://arxiv.org/abs/2608.13568), 2026) achou que o LSP muitas vezes
  *aumenta* o gasto de tokens (+6% a +118% na localização de símbolos), e um rename só por
  localização passou em 2 de 6 tarefas por não ver menções em comentários e strings.

Desenho: consultas semânticas e textuais, e refatorações atômicas no compilador (o rename
também lista as menções textuais).

## Desenho resultante

| comando | o que devolve | por quê |
| --- | --- | --- |
| `check --json` | todos os diagnósticos, causa raiz primeiro, com aplicabilidade | uma rodada corrige vários erros |
| `check --fix` | aplica os `MachineApplicable` e relata | correção trivial sem gastar rodada do modelo |
| `digest <módulo>` | assinaturas, tipos e documentação públicos | índice do projeto no contexto |
| `show <símbolo>` | o corpo de um símbolo e de suas dependências | geração precisa dos corpos |
| `explain <código>` | a página do código de erro | diagnóstico estável e documentado |
| `test --json` | resultado dos blocos `test` | verificação de comportamento |
| `rename`, `refs` | refatoração atômica, menções semânticas e textuais | edição sem número de linha |
| `fmt` | forma canônica | uma forma de escrever |

Diagnósticos dirigidos a hábitos de Python entram como código próprio: mutação de imutável com
`var` sugerido, `raise` com `fail` sugerido, truthiness com a comparação explícita sugerida,
argumento tratado como referência com a convenção `inout` sugerida ([[modelo-de-memoria]]).
A gramática publicada para geração restrita está em [[decodificacao-restrita]].
