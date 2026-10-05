# Piloto de vazamento de Python

O estudo original aponta a falta de corpus como o risco número um: o modelo "escorrega" para
Python. Este piloto mediu isso pela primeira vez, com modelos reais escrevendo na linguagem X
a partir só da especificação. Resultado: com a spec no prompt, o vazamento de Python não
apareceu na sintaxe — apareceu na semântica, e só no modelo menor.

## Método

- **Duas especificações compactas**, `research/pilot/spec-a.md` (variante A) e `spec-b.md`
  (variante B), com cerca de 1.830 tokens cada no `o200k`. Elas diferem só nas construções
  que distinguem as variantes (ver [[sintaxe-da-linguagem-x]]).
- **Dez tarefas** em `research/pilot/tasks.md`, redigidas sem vocabulário de Python
  ("registro", "erro", "nada" em vez de `class`, exceção, `None`): registro, tipo soma,
  parse com três erros, opcional, fila genérica, contagem de palavras, razão contábil com
  erros propagados, interface com duas implementações, árvore de expressões e parse de CSV.
- **Três modelos × duas variantes**: Claude Haiku, Sonnet e Opus, cada um em um agente novo,
  sem contexto anterior, instruído a ler só a spec e as tarefas, escrever cada solução uma vez
  e não revisar. Saídas em `research/pilot/runs/<variante>-<modelo>/`.
- **Três verificações automáticas**, todas com testes em `research/pilot/`:
  - *parse* — gramática Lark de cada variante (`check.py`), validada contra o corpus pareado
    inteiro;
  - *vazamento sintático* — construções de Python fora de strings e comentários (`def`,
    `class`, `raise`, `try`, `except`, decoradores, `Optional[`, `isinstance` e, só na
    variante A, `None`, `lambda`, `import`, `case`, `is`);
  - *vazamento semântico* — percorrendo a árvore (`semantics.py`): reatribuir ou mutar um
    local imutável, mutar `self` sem `var self`, truthiness em valor que não é `bool`, e
    parâmetros `var`.
- **Reprodução:** `uv run --with lark python analyze.py` em `research/pilot/`, que regenera
  `results.md` e `results.json`.

## Resultados

| rodada | programas | parseiam | vazamento sintático | violações semânticas |
| --- | ---: | ---: | ---: | ---: |
| A · Haiku | 10 | 8 | 0 | 1, mais 3 parâmetros `var` |
| A · Sonnet | 10 | 10 | 0 | 0 |
| A · Opus | 10 | 10 | 0 | 0 |
| B · Haiku | 10 | 8 | 0 | 3, mais 3 parâmetros `var` |
| B · Sonnet | 10 | 10 | 0 | 0 |
| B · Opus | 10 | 10 | 0 | 0 |

- **Nenhuma construção sintática de Python em 60 programas**, nas duas variantes e nos três
  modelos — nem `None` na variante A, onde ele é inválido.
- **As quatro falhas de parse são do Haiku**, duas por variante, e nenhuma é Python:
  - `type Queue[T](var items: [T])` — `var` em campo de registro, como em `struct` do Swift;
  - `Negate(Expr)` e `Add(Expr, Expr)` — variantes com campos posicionais, como no Rust e no
    Swift; a spec só mostra campos nomeados.
- **As violações semânticas são hábitos de Python que passam pelo parser**, todas no Haiku:
  - `line = line.strip()` reatribuindo a variável do laço (duas vezes, uma por variante);
  - `items = []` seguido de `items.append(…)` sem `var` (duas vezes, na variante B).
- **O Haiku assumiu semântica de referência nos argumentos.** Na tarefa da razão contábil,
  nas duas variantes, escreveu `fn withdraw(var ledger: {str: int}, …)` e um teste que espera
  ver o `ledger` do chamador alterado. Com semântica de valor ([[modelo-de-memoria]]) o
  parâmetro é uma cópia e o teste falharia. Sonnet e Opus usaram o caminho que a spec oferece: um registro
  `Ledger` com métodos `var self`.

## Duas falhas eram da gramática, não dos modelos

A primeira rodada contou seis falhas. Duas eram defeitos da gramática do piloto: `**` era
lexado como dois `*`, e um literal negativo em padrão (`case Err(Negative(-5))`) não era
aceito. Os dois foram corrigidos com teste antes de recontar. É o fenômeno que a literatura
mede em [[decodificacao-restrita]]: uma gramática incompleta rejeita programas válidos, e
usada para restringir a geração ela derruba a qualidade em vez de protegê-la.

## O que o piloto sustenta

1. **O risco do estudo original muda de lugar.** Com a spec no prompt e a linguagem nomeada,
   modelos Claude não misturaram sintaxe de Python. A literatura vai na mesma direção: nomear
   a linguagem explicitamente levou a adesão a mais de 99% em
   [Moumoula et al.](https://arxiv.org/abs/2503.13620). O que vaza é a semântica — "tudo é
   mutável" e "argumentos são referências" — e isso o parser não pega; só o
   [[compilador-semantico]] pega.
2. **A variante A e a variante B empataram** neste tamanho de amostra (28 de 30 cada). O
   piloto não dá evidência a favor da B no vazamento sintático; a escolha entre elas se apoia
   nos argumentos de semântica de [[sintaxe-da-linguagem-x]] e deve ser decidida pelo
   [[harness-de-avaliacao]].
3. **Os erros que apareceram vêm de outros priors**, Swift e Rust. Aceitar campos posicionais
   em variantes custa pouco e remove uma classe inteira de erro.
4. **A especificação precisa dizer como uma função altera um valor do chamador.** Nem o
   estudo original nem a spec do piloto dizem, e o modelo que precisou disso inventou
   semântica de referência — ver [[modelo-de-memoria]].

## Limitações

- **Uma família de modelos.** Só Claude; modelos abertos menores devem vazar mais, como mostra
  o [[prior-de-treino]].
- **Amostra pequena:** dez tarefas curtas por célula, uma amostra por tarefa, sem controle de
  temperatura. Diferenças de uma ou duas falhas não são sinal.
- **Nada foi executado.** Sem compilador nem transpilador, o piloto mede parse e regras
  estáticas, não corretude funcional. Tipos, exaustividade de `match` e uso de opcional sem
  checagem ficaram de fora.
- **Spec no prompt.** O resultado vale para uso com a spec no contexto, que é o cenário
  previsto pelo estudo original para modelos fechados; não diz nada sobre geração sem ela.
