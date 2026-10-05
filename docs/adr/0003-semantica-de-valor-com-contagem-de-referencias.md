---
status: accepted
---

# 0003 · Semântica de valor com contagem de referências

## Context

A linguagem X precisa de desempenho previsível sem a dificuldade de um borrow checker visível e
sem as pausas de um coletor. O modelo de memória define o runtime, o FFI, a concorrência e o
que o programador (e o modelo) precisa raciocinar — trocá-lo depois é refazer o compilador. A
evidência: Perceus deixou o Koka a menos de 10% do C++ numa árvore rubro-negra; o Lean 4 com
reuso e empréstimo inferido foi 5 vezes mais rápido que o OCaml num benchmark; a semântica de
valor mutável com referências de segunda classe elimina ciclos por construção. Os custos: a
contagem atômica custou até 59% no Perceus, e o Swift precisou pôr convenções de parâmetro na
linguagem porque a inferência para nas fronteiras de ABI. Ver [[modelo-de-memoria]].

## Decision

Semântica de valor mutável: atribuir ou passar é conceitualmente copiar; o compilador troca por
move ou empréstimo quando prova que é seguro, e coleções usam copy-on-write. Memória gerida por
contagem de referências com reuso no estilo Perceus e inferência de empréstimo no estilo Lean,
contagem não atômica dentro de uma tarefa, e valores copiados ou movidos entre tarefas.
Referências nunca são armazenadas, então não há ciclos. Convenções de parâmetro explícitas
(padrão, `inout`, `sink`) com marcador visível na chamada. Rejeitados: coletor por rastreamento
(pausas e heap maior), ownership com borrow checker no estilo Rust (mais rodadas de compilação
para o modelo) e gerência manual.

## Consequences

- Grafos e estruturas com compartilhamento usam arena e índices.
- `inout` com marcador na chamada é sintaxe nova para o modelo; o piloto mostrou que, sem ela,
  um modelo inventa semântica de referência, então a convenção entra já na v1.
- Código que aloca muito tende a ficar mais lento que o numérico (Swift: 10,5× o C em
  binary-trees); os benchmarks devem relatar essa classe à parte.
- No destino Python, a semântica de valor é emulada por cópia ao entrar num `var` ou `inout`.
