#[derive(Debug)]
pub enum CodeLength {
    Length(u8),
    CopyPrevious(u8),
    RepeatZero3Bits(u8),
    RepeatZero7Bits(u8),
}

const MIN_REPEAT: u8 = 3;

pub fn encode_lengths(lengths: &[u8]) -> Option<Vec<CodeLength>> {
    let mut out = Vec::new();
    let mut prev = 19;
    let mut repeat = 0;
    let mut iter = lengths.iter().enumerate().peekable();
    while let Some((n, l)) = iter.next() {
        if *l == prev && repeat < 6 && iter.peek().is_some() {
            repeat += 1;
        } else {
            if repeat >= MIN_REPEAT {

                println!("Writing repeat? n: {}, repeat: {}, l: {}, prev: {}",
                         n,
                         repeat,
                         *l,
                         prev);
                let ret = match *l {
                    0 => {
                        if repeat <= 10 {
                            CodeLength::RepeatZero3Bits(repeat)
                        } else {
                            CodeLength::RepeatZero7Bits(repeat)
                        }
                    }
                    1...15 => CodeLength::CopyPrevious(repeat),
                    _ => return None,
                };
                println!("Repeat: {:?}", ret);
                out.push(ret);
                repeat = 1;
                if *l != prev || iter.peek().is_none() {
                    println!("Length: {}", l);
                    out.push(CodeLength::Length(*l));
                    repeat = 0;
                }
            } else {
                println!("n: {}, repeat: {}, l: {}", n, repeat, *l);
                let mut i = repeat as i32;
                while i >= 0 {
                    out.push(CodeLength::Length(lengths[n as usize - i as usize]));
                    i -= 1;
                }
                repeat = 0;
            }
            // repeat = 0;
        }
        prev = *l;
    }
    Some(out)
}

#[derive(Copy, Clone, Debug)]
pub struct ChainNode {
    weight: usize,
    count: usize,
    tail: Option<usize>,
}

fn advance_lookahead(indexes: &mut [(usize, usize)], index: usize, next: usize) {
    indexes[index].0 = indexes[index].1;
    indexes[index].1 = next;
}

/// Implementation of boundary package merge algorithm described by Katajainen/Moffat/Turpin in
/// "A Fast and Space-Economical Algorithm for Length-Limited Coding"
pub fn boundary_package_merge(lookahead_indexes: &mut [(usize, usize)],
                              nodes: &mut Vec<ChainNode>,
                              index: usize,
                              num_leaves: usize,
                              last: bool) {

    let count = nodes[lookahead_indexes[index].1].count;
    let next_count = count + 1;
    println!("Count: {}, index: {}", count, index);
    //    println!("First list: {:?}", lists[0]);
    if index == 0 && next_count >= num_leaves {
        return;
    };

    if index == 0 {
        // If we are at index 0, we need to move the lookahead to the next leaf node
        advance_lookahead(lookahead_indexes, index, nodes.len());

        let new_weight = nodes[next_count].weight;

        nodes.push(ChainNode {
            weight: new_weight,
            count: next_count,
            tail: None,
        });
        return;
    }

    let sum = {
        let la = lookahead_indexes[index - 1];
        nodes[la.0].weight + nodes[la.1].weight
    };

    //If the sum of the two lookahead nodes in the previous list is greater than the next leaf node,
    //we add a new package, otherwise, we add another lookahead node
    if next_count < num_leaves && sum > nodes[next_count].weight {
        let y = nodes[lookahead_indexes[index].1].tail;

        advance_lookahead(lookahead_indexes, index, nodes.len());

        let next_weight = nodes[next_count].weight;

        nodes.push(ChainNode {
            weight: next_weight,
            count: next_count,
            tail: y,
        });
    } else {
        {
            advance_lookahead(lookahead_indexes, index, nodes.len());

            nodes.push(ChainNode {
                weight: sum,
                count: count,
                tail: Some(lookahead_indexes[index - 1].1),
            });
        }
        if !last {
            //If we add a package, we need to run boundary_pm on the previous lists to look for
            //more leaf nodes
            //We might want to avoid using recursion here, though we won't ever go more than 15
            //levels in as that is the maximum code length allowed by the deflate spec.
            boundary_package_merge(lookahead_indexes, nodes, index - 1, num_leaves, false);
            boundary_package_merge(lookahead_indexes, nodes, index - 1, num_leaves, false);
        }
    }
}

pub fn huffman_lengths_from_frequency(frequencies: &[usize], max_len: usize) -> Vec<usize> {
    let num_leaves = frequencies.len();

    // Create a vector of nodes used by the package merge algorithm
    let mut nodes: Vec<_> = frequencies.iter()
        .enumerate()
        .map(|(n, f)| {
            ChainNode {
                weight: *f,
                count: n,
                tail: None,
            }
        })
        .collect();

    nodes.sort_by(|a, b| a.weight.cmp(&b.weight));

    let mut lookahead_ptrs = vec![(0, 1); max_len];

    let num_runs = (2 * frequencies.len()) - 2 - 2;
    for i in 0..num_runs {
        let last = i == num_runs - 1;
        boundary_package_merge(&mut lookahead_ptrs,
                               &mut nodes,
                               max_len - 1,
                               num_leaves,
                               last);
    }

    let mut lengths = vec![0; frequencies.len()];
    let head = nodes.len() - 1;
    //    println!("Nodes:\n{:?}", nodes);
    let mut next_node = head;
    loop {
        let node = nodes[next_node];
        //        println!("Next_node {}, node: {:?}", next_node, node);

/*
        for i in 0..node.count + 1 {
            lengths[i] += 1;
        }
         */
        for item in lengths.iter_mut().take(node.count + 1) {
            *item += 1;
        }
        if let Some(n) = node.tail {
            next_node = n;
        } else {
            break;
        }
    }
    lengths
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_encode_lengths() {
        use huffman_table::FIXED_CODE_LENGTHS;
        let enc = encode_lengths(&FIXED_CODE_LENGTHS).unwrap();
        println!("{:?}", enc);
        println!("Number of 7s: {}",
                 FIXED_CODE_LENGTHS.iter().filter(|x| **x == 9).count());
        panic!();
    }

    #[test]
    fn test_lengths_from_frequencies() {
        let frequencies = [1, 1, 5, 7, 10, 14];

        let expected = vec![4, 4, 3, 2, 2, 2];
        let res = huffman_lengths_from_frequency(&frequencies, 4);

        assert_eq!(&expected, &res);
    }
}
