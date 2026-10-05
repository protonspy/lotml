# Custo em tokens

Um LLM lê e escreve tokens, não caracteres. Esta página reúne o que foi medido sobre o custo
em tokens da [[sintaxe-da-linguagem-x]] e o que a literatura diz sobre comprimir linguagens.
Conclusão: a economia que a sintaxe consegue é de cerca de 10% contra Python tipado — metade
da meta do estudo original — e em uso agêntico ela pesa menos que uma única rodada de correção.

## Como foi medido

- **Oito tokenizadores, seis famílias:** `o200k` e `cl100k` (OpenAI, via tiktoken), Llama 3,
  Qwen3, DeepSeek-V3, Gemma 3, Mistral Nemo e StarCoder2 (Hugging Face).
  Llama 3 e Gemma 3 vêm dos espelhos `unsloth/*`, porque os repositórios oficiais exigem
  aceite de licença; os nomes exatos estão em `research/tokens/counter.py`.
- **Tokens especiais ficam fora.** Os tokenizadores de Llama, Gemma e Mistral somam um BOS por
  padrão; o teste `research/tokens/test_counter.py` exige contagem zero para texto vazio.
- **O tokenizador do Claude não foi medido.** A Anthropic não o publica e a contagem exige a
  API `count_tokens` com chave, que não estava disponível. É uma lacuna desta medição.
- **Corpus pareado** em `research/tokens/corpus/`: 12 tarefas (registros, tipos soma, erros,
  genéricos, traits, algoritmos), cada uma em Python tipado moderno (3.12, `X | None`,
  `type`, dataclasses), na variante A e na variante B, sempre com testes. O lado Python passa
  no pytest e no `mypy --strict`.
- **Reprodução:** `measure.py` em `research/tokens/` regenera `results.md` e `results.json`;
  os comandos, com versões e revisões fixadas, estão em `research/README.md`.

## Resultado: cerca de 10% a menos que Python tipado

| tokenizador | Python tipado | variante A | variante B | A / Python | B / Python |
| --- | ---: | ---: | ---: | ---: | ---: |
| o200k | 2594 | 2348 | 2368 | 0,905 | 0,913 |
| cl100k | 2605 | 2347 | 2367 | 0,901 | 0,909 |
| llama3 | 2605 | 2347 | 2367 | 0,901 | 0,909 |
| qwen3 | 2622 | 2379 | 2399 | 0,907 | 0,915 |
| deepseek-v3 | 2715 | 2420 | 2440 | 0,891 | 0,899 |
| gemma3 | 3025 | 2729 | 2748 | 0,902 | 0,908 |
| mistral-nemo | 2718 | 2445 | 2466 | 0,900 | 0,907 |
| starcoder2 | 2860 | 2599 | 2618 | 0,909 | 0,915 |

- **Linhas exageram o ganho.** Linhas não vazias caem 28% (298 para 215), caracteres 15%
  (9340 para 7911), tokens só 9 a 11%.
- **O ganho varia muito por tarefa:** de 0,76 (pilha genérica, onde o Python precisa de
  `field(default_factory=list)`) a 0,99 (busca binária e transferência bancária, que são quase
  só fluxo de controle).
- **O exemplo do próprio estudo original** cai de 16 para 6 linhas (−62%), mas de 102 para 87
  tokens no `o200k` — entre 12% e 16% conforme o tokenizador. "Menos da metade das linhas" é
  verdade; em tokens, a queda é bem menor.

## De onde vem a economia, e onde ela se perde

Medido construção a construção, em contexto de linha (`research/tokens/results.md`, seção
*Isolated constructs*). Diferença mediana entre os oito tokenizadores:

| construção | forma de referência | forma proposta | Δ tokens |
| --- | --- | --- | ---: |
| registro de 2 campos | `@dataclass class User: …` | `type User(name: str, age: int)` | −7 |
| registro imutável | `@dataclass(frozen=True) …` | `type P(x: f64, y: f64)` | −7 |
| campo opcional | `Optional[str]` ou `str \| None` | `str?` | −1 |
| lambda | `lambda p: p.age` | `p => p.age` | −1 |
| braço de `match` | `case Circle(r):` | `Circle(r):` | −1 |
| palavra de função | `def` | `fn` | 0 |
| lista, dicionário, tupla | `list[int]`, `dict[str, int]` | `[int]`, `{str: int}` | 0 |
| import | `from a.b import c, d` | `use a.b.{c, d}` | 0 |
| padrão de opcional | `y or 0` | `y ?? 0` | 0 |
| mutável | `x = 1` | `var x = 1` | +1 |
| bloco | indentação | chaves | +1 |
| assinatura que falha | `-> User` (exceção oculta) | `-> User ! LookupErr` | +3 |
| teste de opcional | `if x:` | `if x is not None:` | +3 |
| teste de lista vazia | `if xs:` | `if len(xs) > 0:` | +5,5 |

- **A economia vem de apagar declarações:** dataclass, classes de exceção,
  `field(default_factory=…)`, `frozen=True`. Encurtar palavras-chave não rende nada.
- **`Optional[str]` em contexto custa o mesmo que `str | None`**, e `str?` economiza um
  token. A afirmação "1 token em vez de 3–4" do estudo original não se sustenta.
- **A proibição de truthiness é a regra mais cara por ocorrência.** Ela continua certa — ver
  [[sistema-de-tipos]] — mas tem preço, e a biblioteca padrão pode baixá-lo com algo como
  `xs.is_empty()`.
- **Tornar o erro visível na assinatura custa tokens** (+3), porque o Python esconde a exceção.
  É o preço da informação, e é exatamente a informação que o [[compilador-semantico]] usa.

## Abreviações e símbolos

| palavra | tokens | forma curta | tokens |
| --- | --- | --- | --- |
| `count`, `index`, `value`, `message` | 1 | `cnt`, `idx`, `val`, `msg` | 1 |
| `number`, `result`, `buffer` | 1 | `num`, `res`, `buf` | 1 |
| `return` | 1 | `rtn` | 1–2 |
| `->`, `lambda` | 1 | `→`, `λ` | 1 |
| `<=`, `!=`, `not` | 1 | `≤`, `≠`, `¬` | 1–2 |
| `reshape`, `reverse` | 1–2 | `⍴`, `⌽` | 2–4 |
| `outer_product` | 2–3 | `∘.×` | 4 |

Faixas mínimo–máximo entre os oito tokenizadores, com espaço à frente. As abreviações comuns
em código custam um token, igual à palavra inteira; só glifos raros (APL) custam mais. O
motivo para evitar abreviações é legibilidade e prior de treino, não tokens.

## Indentação

Entre 7% e 8% dos tokens do corpus são espaço no início da linha, tanto no Python quanto na
variante A. O StarCoder2 dá um valor negativo porque seu tokenizador funde o espaço ao token
seguinte, então remover a indentação não encurta a contagem. Trocar indentação por chaves
custa um token por bloco: a escolha entre as duas não é uma questão de tokens, e o argumento
que decide está em [[sintaxe-da-linguagem-x]].

## O que a literatura diz

- **SimPy** — Sun et al., [*AI Coders Are Among Us*](https://arxiv.org/abs/2404.16333),
  ISSTA 2024. Uma gramática do Python reescrita para máquinas (sem `def`, sem `:`, blocos por
  marcadores) economiza 10,4% no tokenizador do GPT-4, 13,5% no do CodeLlama e 8,6% no do
  StarCoder. Os modelos precisaram ser treinados nela: treinar só em SimPy derrubou o
  TinyLlama de 10,00% para 5,91% de pass@1. Nenhum teste em modelos de fronteira. Mesmo
  abrindo mão da legibilidade, o ganho fica em 9–14%, a mesma ordem da variante A.
- **Token Sugar** — Sun et al., [arXiv 2512.08266](https://arxiv.org/abs/2512.08266), ASE
  2025. Atalhos minerados reduzem até 15,1% dos tokens, também exigindo treino.
- **Alderson** — [*Which programming languages are most token-efficient?*](https://martinalderson.com/posts/which-programming-languages-are-most-token-efficient/),
  2026, só com o tokenizador do GPT-4 e "not a scientific study". Linguagens dinâmicas saem na
  frente; inferência de tipos (Haskell, F#) chega perto; glifos APL custam vários tokens.
- **Tokenmaxxing** — Wu, Anderson e Guha, [arXiv 2607.22807](https://arxiv.org/abs/2607.22807),
  2026. Em uso agêntico, o custo em relação ao Python foi 1,28–1,69× em OCaml e 1,07–1,57× em
  Rust, puxado por loops em código que não compila e por protótipos em Python — não por
  verbosidade de superfície.
- **Dan Luu** — [*How does programming language affect token efficiency and correctness?*](https://danluu.com/pl-tokens/).
  Contagem de tokens em tarefas triviais não previu o custo agêntico em tarefas realistas.
- **Ustynov** — [arXiv 2604.07502](https://arxiv.org/abs/2604.07502), 2026. Um formato de log
  abreviado cortou 17,1% dos tokens de entrada e aumentou 67,2% o custo total da sessão (dados
  de log, uma sessão por condição): comprimir pode deslocar o custo para o raciocínio.

## Conclusões

1. **A meta "≥20% menos tokens que o Python equivalente" não é alcançável por sintaxe** sem
   abrir mão do prior de treino: a variante A dá cerca de 10% contra Python tipado, e o SimPy,
   que abre mão da legibilidade, dá 9–14%.
2. **A variante B custa 0,8 ponto percentual a mais que a A.** Alinhar ao Python é quase
   grátis em tokens; o que ela compra está em [[piloto-de-vazamento-de-python]].
3. **O custo agêntico é dominado por rodadas de correção.** Uma rodada evitada vale mais que
   toda a economia de sintaxe, então a métrica primária é compilar de primeira e a meta de
   tokens vira "não pior que Python tipado" — ver [[requisitos-e-roadmap]].
4. **Limitações.** Um único autor escreveu o corpus conhecendo as hipóteses; a biblioteca
   padrão assumida difere da do Python em alguns pontos (`find`, `pop` que devolve opcional,
   `Heap`); e a escolha das tarefas move o resultado entre 0,76 e 0,99. O
   [[harness-de-avaliacao]] deve medir código gerado por modelos, não escrito à mão.
