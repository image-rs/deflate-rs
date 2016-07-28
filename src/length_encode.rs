/// An enum representing the different types in the run-length encoded data used to encode
/// huffman table lenghts
#[derive(Debug)]
pub enum EncodedLength {
    // An actual length value
    Length(u8),
    // Copy the previous value n times
    CopyPrevious(u8),
    // Repeat zero n times (with n represented by 3 bits)
    RepeatZero3Bits(u8),
    // Repeat zero n times (with n represented by 7 bits)
    RepeatZero7Bits(u8),
}

const MIN_REPEAT: u8 = 3;

/// Run-length encodes the lenghts of the values in `lenghts` according to the deflate
/// specification. This is used for writing the code lenghts for the huffman table for
/// in the deflate stream.
pub fn encode_lengths(lengths: &[u8]) -> Option<Vec<EncodedLength>> {
    let mut out = Vec::new();
    let mut prev = 19;
    let mut repeat = 0;
    let mut iter = lengths.iter().enumerate().peekable();
    while let Some((n, l)) = iter.next() {
        if *l == prev && repeat < 6 && iter.peek().is_some() {
            repeat += 1;
        } else if repeat >= MIN_REPEAT {

            println!("Writing repeat? n: {}, repeat: {}, l: {}, prev: {}",
                     n,
                     repeat,
                     *l,
                     prev);
            let ret = match *l {
                0 => {
                    if repeat <= 10 {
                        EncodedLength::RepeatZero3Bits(repeat)
                    } else {
                        EncodedLength::RepeatZero7Bits(repeat)
                    }
                }
                1...15 => EncodedLength::CopyPrevious(repeat),
                _ => return None,
            };
            println!("Repeat: {:?}", ret);
            out.push(ret);
            repeat = 1;
            if *l != prev || iter.peek().is_none() {
                println!("Length: {}", l);
                out.push(EncodedLength::Length(*l));
                repeat = 0;
            }
        } else {
            println!("n: {}, repeat: {}, l: {}", n, repeat, *l);
            let mut i = repeat as i32;
            while i >= 0 {
                out.push(EncodedLength::Length(lengths[n as usize - i as usize]));
                i -= 1;
            }
            repeat = 0;

        }
        prev = *l;
    }
    Some(out)
}

type NodeIndex = usize;

/// A struct representing a node used in the package-merge algorithm
#[derive(Copy, Clone, Debug)]
pub struct ChainNode {
    // The weight of the node, which in this case is the frequency of the symbol in the input
    // data we are creating huffman codes for
    weight: usize,
    // Number of leaf nodes to the left of this node
    // In this case, the count is equal to the symbol in the input data this node represents
    count: u16,
    // A pointer to the previous node in this chain, if it exists
    // As using actual pointers in rust would involve unsafe code, the tail is represented by an
    // index to the vector containing all the nodes. (This comes with the added benefit of being
    // able to use a smaller type than a pointer, potentially saving some memory)
    tail: Option<NodeIndex>,
}

fn advance_lookahead(indexes: &mut [(usize, usize)], index: usize, next: usize) {
    indexes[index].0 = indexes[index].1;
    indexes[index].1 = next;
}

/// Implementation of boundary package merge algorithm described by Katajainen/Moffat/Turpin in
/// "A Fast and Space-Economical Algorithm for Length-Limited Coding"
pub fn boundary_package_merge(lookahead_indexes: &mut [(usize, usize)],
                              nodes: &mut Vec<ChainNode>,
                              leaves: &[ChainNode],
                              index: usize,
                              last: bool) {

    let count = nodes[lookahead_indexes[index].1].count;
    let next_count = count + 1;
    // println!("Count: {}, index: {}", count, index);
    //    println!("First list: {:?}", lists[0]);
    if index == 0 && count >= leaves.len() as u16 {
        // num_leaves {
        return;
    };

    if index == 0 {
        // If we are at index 0, we need to move the lookahead to the next leaf node
        advance_lookahead(lookahead_indexes, index, nodes.len());

        let new_weight = leaves[count as usize].weight;

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

    // If the sum of the two lookahead nodes in the previous list is greater than the next leaf
    // node, we add a new package, otherwise, we add another lookahead node
    if count < leaves.len() as u16 && sum > leaves[count as usize].weight {

        let y = nodes[lookahead_indexes[index].1].tail;

        let next_weight = leaves[count as usize].weight;

        advance_lookahead(lookahead_indexes, index, nodes.len());

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
            // If we add a package, we need to run boundary_pm on the previous lists to look for
            // more leaf nodes
            // We might want to avoid using recursion here, though we won't ever go more than 15
            // levels in as that is the maximum code length allowed by the deflate spec.
            boundary_package_merge(lookahead_indexes, nodes, leaves, index - 1, false);
            boundary_package_merge(lookahead_indexes, nodes, leaves, index - 1, false);
        }
    }
}

pub struct CodeLength {
    length: u8,
    symbol: u16,
}

pub fn huffman_lengths_from_frequency(frequencies: &[usize], max_len: usize) -> Vec<usize> {
    // Make sure the number of frequencies is sensible since we use u16 to index.
    assert!(frequencies.len() < u16::max_value() as usize);
    assert!(max_len > 1 && max_len < 16);

    let mut lengths = vec![0; frequencies.len()];

    // Create a vector of nodes used by the package merge algorithm
    // We start by adding a leaf node for each nonzero frequency, and subsequently sorting them by weight.
    let mut leaves: Vec<_> = frequencies.iter()
        .enumerate()
        .filter_map(|(n, f)| {
            if *f > 0 {
                Some(ChainNode {
                    weight: *f,
                    count: n as u16,
                    tail: None,
                })
            } else {
                None
            }
        })
        .collect();

    // Special case with zero or 1 value having a non-zero frequency (this will break the package merge otherwise)
    if leaves.len() == 1 {
        lengths[leaves[0].count as usize] += 1;
        return lengths;
    } else if leaves.is_empty() {
        return lengths;
    }

    leaves.sort_by(|a, b| a.weight.cmp(&b.weight));

    // We create the two first lookahead nodes from the two first leaves, with counts 1 and 2
    let mut nodes: Vec<_> = leaves.iter()
        .take(2)
        .enumerate()
        .map(|(n, f)| {
            ChainNode {
                weight: f.weight,
                count: n as u16 + 1,
                tail: None,
            }
        })
        .collect();

    // Indexes to the current lookahead nodes in each list
    // The lookahead indexes in each list start out pointing to the first two leaves.
    let mut lookahead_ptrs = vec![(0, 1); max_len];

    // The boundary package-merge algorhithm is run repeatedly until we have 2n - 2 nodes in the
    // last list, which tells us how many active nodes there are in each list.
    let num_runs = (2 * leaves.len()) - 2 - 2;
    for i in 0..num_runs {
        let last = i == num_runs - 1;
        boundary_package_merge(&mut lookahead_ptrs, &mut nodes, &leaves, max_len - 1, last);
    }

    let mut lengths = vec![0; frequencies.len()];
    let head = lookahead_ptrs[lookahead_ptrs.len() - 1].1;

    let mut next_node = head;
    loop {
        let node = nodes[next_node];

        for item in leaves.iter().take(node.count as usize) {
            lengths[item.count as usize] += 1;
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
        // TODO: Write a proper test for this
        use huffman_table::FIXED_CODE_LENGTHS;
        let enc = encode_lengths(&FIXED_CODE_LENGTHS).unwrap();
        println!("{:?}", enc);
        println!("Number of 7s: {}",
                 FIXED_CODE_LENGTHS.iter().filter(|x| **x == 9).count());
        // panic!();
    }

    #[test]
    fn test_lengths_from_frequencies() {
        let frequencies = [1, 1, 5, 7, 10, 14];

        let expected = [4, 4, 3, 2, 2, 2];
        let res = huffman_lengths_from_frequency(&frequencies, 4);

        assert_eq!(expected, res.as_slice());

        let frequencies = [1, 5, 1, 7, 10, 14];
        let expected = [4, 3, 4, 2, 2, 2];

        let res = huffman_lengths_from_frequency(&frequencies, 4);

        assert_eq!(expected, res.as_slice());

        let frequencies = [0, 25, 0, 10, 2, 4];

        let res = huffman_lengths_from_frequency(&frequencies, 4);
        assert_eq!(res[0], 0);
        assert_eq!(res[2], 0);
        assert!(res[1] < 4);

        // Only one value
        let frequencies = [0, 0, 0, 0, 0, 0, 0, 0, 55, 0, 0, 0];
        let res = huffman_lengths_from_frequency(&frequencies, 5);
        let expected = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0];
        assert_eq!(expected, res.as_slice());

        // No values
        let frequencies = [0; 30];
        let res = huffman_lengths_from_frequency(&frequencies, 5);
        assert_eq!(frequencies, res.as_slice());
    }
}
