# Estratégia de transpilação

O estudo original recomenda transpilar primeiro para Python (validar o design em semanas),
depois para C (atingir a meta de desempenho) e só então escrever backend nativo; e,
separadamente, transpilar Python para a linguagem lotml para gerar corpus. A ordem se sustenta. A
evidência acrescenta o preço de cada lacuna semântica entre a linguagem lotml e o Python, e uma
consequência que o estudo não tirou: o transpilador para Python é o que permite ao harness
executar programas.

## Python como primeiro destino

**Inteiros.** O Python tem inteiros de precisão arbitrária; a linguagem lotml tem `i64` com overflow
definido. Todo compilador de Python diverge aqui: o Cython mantém o `int` como objeto Python
porque o C "can be quite different with respect to overflow and division", o Codon usa 64 bits,
e o mypyc deixa o overflow de `i64` indefinido. Emular trap custa uma verificação por operação
— numa medição local e grosseira feita durante esta pesquisa (CPython 3.13, Windows), cerca de
2,2× numa soma, e 3,4× para emular wrap. Na fase 1 o destino Python serve para validar o
design, não para desempenho, então o trap emulado é aceitável. A semântica tem de ser uma só
— ver [[sistema-de-tipos]].

**Semântica de valor.** Listas e dicionários do Python são referências, e não há lista
copy-on-write embutida. Como tudo é imutável por padrão, só precisa de cópia o valor que entra
num `var` ou num parâmetro `inout`/`sink` ([[modelo-de-memoria]]); imutáveis podem ser
compartilhados porque o compilador garante que ninguém os altera. Alternativas mais caras:
estruturas persistentes (pyrsistent, acesso log32) ou o `frozendict` do 3.15 (PEP 814).

**Concorrência.** Não há green threads fiéis no Python; como é v2, tarefas viram threads —
ver [[concorrencia-sem-cor]].

**Erros apontando para o fonte.** Nós do módulo `ast` do Python carregam `lineno` e
`col_offset`, e `compile()` aceita uma AST. Emitir AST com as posições originais deve fazer os
tracebacks apontarem para o arquivo `.x` — inferência a partir da documentação, a verificar no
harness. O Hy compila para AST do Python, e o Coconut preserva números de linha com
`--line-numbers`. Haxe e Coconut também têm o Python como destino.

## O ecossistema Python visto da linguagem lotml

- **Nenhuma linguagem gera bindings tipados a partir dos stubs `.pyi`.** O Erg ignora as dicas
  de tipo do Python e exige declarações à mão, o Codon pede assinaturas manuais, o Mojo trata
  objetos Python como dinâmicos. Gerar bindings a partir do typeshed (que mypy, pyright,
  PyCharm, Pyrefly e ty consomem) é uma oportunidade, com risco de deriva: o typeshed avisa que
  "any version bump can introduce changes".
- **Stubs não declaram exceções.** Logo, toda chamada para Python pode falhar, e o binding
  gerado devolve `T ! PyError`. É o preço honesto de ter erros no tipo.

## C como segundo destino

- **Precedentes:** Nim (C, C++, Objective-C e JS), Koka (C com mimalloc e Perceus, sem coletor),
  Lean 4 (C), Vala (C/GObject, contagem com `weak`), Chicken Scheme (CPS sobre a pilha do C).
- **Mapeamento de erros:** diretivas `#line`, feitas para geradores como o bison devolverem
  erros e depurador ao fonte original; o Nim as emite com `--lineDir`.
- **Overflow com sinal é comportamento indefinido em C**, e o compilador dobra `(a+1)>a` para
  verdadeiro. O gerador tem de emitir builtins de aritmética verificada (ou `-ftrapv`).
- **Rust como destino continua não recomendado**, como no estudo original.

## Tempo de compilação

- **O backend pesa menos do que parece.** No rustc, o Cranelift reduziu cerca de 20% do tempo de
  geração de código, o que dá cerca de 5% de uma compilação limpa; a LWN mediu −20% de tempo de
  parede num build de debug.
- **O frontend incremental pesa mais.** O Roc reescreveu o compilador de Rust para Zig (487 dias
  até a paridade) e reconstrói 450 mil linhas em cerca de 35 ms, contra 3,4 s antes. O Salsa
  (0.28.5, ainda rotulado "experimental") sustenta o rust-analyzer com a invariante "typing
  inside a function's body never invalidates global derived data" — a propriedade que a meta de
  checagem em menos de 100 ms exige.

## Python para a linguagem lotml: o corpus

O caminho medido é o do [MultiPL-T](https://arxiv.org/abs/2308.09895): traduzir com um LLM e
manter só o que passa nos testes ([[prior-de-treino]]). O corpus pareado mostra que, de Python
tipado para a linguagem lotml, boa parte do mapeamento é mecânica (dataclass vira registro, classes
de exceção viram tipo soma, `Optional` vira `T?`). O que não é mecânico é decidir quais funções
falham: exige seguir os `raise` transitivamente. O desenho viável é híbrido — regras para o
mecânico, LLM para o resto, testes para validar.

## Ordem recomendada

A ordem do estudo original se mantém, com uma antecipação: um transpilador mínimo para Python
entra já na fase 0, porque sem executar programas o [[harness-de-avaliacao]] só mede parse e
regras estáticas, como o [[piloto-de-vazamento-de-python]]. O mesmo transpilador, crescido, é
a fase 1.
