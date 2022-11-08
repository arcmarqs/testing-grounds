
mod node;
mod log;

use node::Node;
fn main() {
    let mut node = Node::new(1);
    node.process_operation(node::Operation::Insert(String::from("Teste")));

    node.print_data();

    node.print_log();

    node.process_operation(node::Operation::Insert(String::from("Teste 2")));

    node.print_data();

    node.print_log();

    node.process_operation(node::Operation::Insert(String::from("Teste 3")));

    node.print_data();

    node.print_log();

    node.process_operation(node::Operation::Remove(String::from("Teste 2")));

    node.print_data();

    node.print_log();

}
